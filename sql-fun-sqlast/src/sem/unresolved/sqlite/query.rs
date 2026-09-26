use crate::sem::unresolved::{
    Alias, Call, Cte, CteBody, DerivedTable, Expr, Frame, FrameBound, FrameExclusion, FrameMode,
    FromGroup, FromItem, Grouping, Join, JoinKind, Name, Order, Query, QueryBody, Relation, Select,
    SetOperation, SetQuery, TableFunction, TableFunctions, Target, Window, With,
};
use crate::{sem::AnalysisError, syn::sqlite as syn};

impl From<QueryBody> for Query {
    fn from(body: QueryBody) -> Self {
        Self {
            with: With::default(),
            body,
            order: Vec::new(),
            limit: None,
            offset: None,
            with_ties: false,
            locks: Vec::new(),
        }
    }
}

impl Query {
    pub(crate) fn sqlite(value: &syn::Select) -> Result<Self, AnalysisError> {
        let mut query = Self::from(QueryBody::sqlite(value.body().select())?);
        for compound in value.body().compounds() {
            let right = Self::from(QueryBody::sqlite(compound.select())?);
            query = Self::from(QueryBody::sqlite_compound(
                compound.operator(),
                query,
                right,
            ));
        }
        query.with = With::sqlite(value.with().as_ref())?;
        query.order = Order::sqlite_list(value.order_by())?;
        if let Some(limit) = value.limit() {
            query.limit = Some(Expr::sqlite(limit.expr())?);
            query.offset = limit.offset().as_deref().map(Expr::sqlite).transpose()?;
        }
        Ok(query)
    }
}

impl QueryBody {
    fn sqlite(value: &syn::OneSelect) -> Result<Self, AnalysisError> {
        Ok(match value {
            syn::OneSelect::Values(rows) => {
                let mut values = Vec::new();
                for row in rows {
                    let mut expressions = Vec::new();
                    for expression in row {
                        expressions.push(Expr::sqlite(expression)?);
                    }
                    values.push(expressions);
                }
                Self::Values(values)
            }
            syn::OneSelect::Select(value) => {
                let mut targets = Vec::new();
                for target in value.columns() {
                    targets.push(Target::sqlite(target)?);
                }
                let mut windows = Vec::new();
                for window in value.window_clause() {
                    let mut common = Window::sqlite(window.window())?;
                    common.name = Some(window.name().value().to_owned());
                    windows.push(common);
                }
                let (group, having) = if let Some(group) = value.group_by() {
                    (
                        Grouping::sqlite_list(group.exprs())?,
                        group.having().as_deref().map(Expr::sqlite).transpose()?,
                    )
                } else {
                    (Vec::new(), None)
                };
                Self::Select(Box::new(Select {
                    into: None,
                    distinct: if matches!(value.distinctness(), Some(syn::Distinctness::Distinct)) {
                        Some(Vec::new())
                    } else {
                        None
                    },
                    targets,
                    from: value
                        .from()
                        .as_ref()
                        .map(FromItem::sqlite_from)
                        .transpose()?
                        .unwrap_or_default(),
                    predicate: value
                        .where_clause()
                        .as_deref()
                        .map(Expr::sqlite)
                        .transpose()?,
                    group,
                    group_distinct: false,
                    having,
                    windows,
                }))
            }
        })
    }

    fn sqlite_compound(operator: &syn::CompoundOperator, left: Query, right: Query) -> Self {
        Self::Set(Box::new(SetQuery {
            operation: SetOperation::sqlite(operator),
            all: matches!(operator, syn::CompoundOperator::UnionAll),
            left,
            right,
        }))
    }
}

impl SetOperation {
    const fn sqlite(value: &syn::CompoundOperator) -> Self {
        match value {
            syn::CompoundOperator::Union | syn::CompoundOperator::UnionAll => Self::Union,
            syn::CompoundOperator::Except => Self::Except,
            syn::CompoundOperator::Intersect => Self::Intersect,
        }
    }
}

impl Target {
    fn sqlite(value: &syn::ResultColumn) -> Result<Self, AnalysisError> {
        match value {
            syn::ResultColumn::Expr(value) => Ok(Self {
                expression: Expr::sqlite(value.expr())?,
                alias: value.as_().as_ref().map(alias_name),
            }),
            syn::ResultColumn::Star => Ok(Self {
                expression: Expr::star(&Name::default()),
                alias: None,
            }),
            syn::ResultColumn::TableStar(name) => Ok(Self {
                expression: Expr::star(&Name::local(name.value())),
                alias: None,
            }),
        }
    }
}

impl Expr {
    fn star(name: &Name) -> Self {
        Self::Star(name.clone())
    }
    fn table_call(name: &Name, arguments: &[Self]) -> Self {
        let mut call = Call::sqlite_base("");
        call.name = name.clone();
        call.arguments = arguments.to_vec();
        Self::Call(Box::new(call))
    }
}

fn alias_name(value: &syn::As) -> String {
    match value {
        syn::As::As(name) | syn::As::Elided(name) | syn::As::ImplicitColumnName(name) => {
            name.value().to_owned()
        }
    }
}

impl Alias {
    fn sqlite(value: &Option<syn::As>) -> Option<Self> {
        value.as_ref().map(|alias| Self {
            name: alias_name(alias),
            columns: Vec::new(),
        })
    }
}

impl Grouping {
    fn sqlite_list(values: &[syn::Expr]) -> Result<Vec<Self>, AnalysisError> {
        let mut groups = Vec::new();
        for value in values {
            groups.push(Self::Expression(Expr::sqlite(value)?));
        }
        Ok(groups)
    }
}

impl Order {
    pub(super) fn sqlite_list(values: &[syn::SortedColumn]) -> Result<Vec<Self>, AnalysisError> {
        let mut order = Vec::new();
        for value in values {
            order.push(Self {
                expression: Expr::sqlite(value.expr())?,
                descending: value
                    .order()
                    .map(|order| matches!(order, syn::SortOrder::Desc)),
                nulls_first: value
                    .nulls()
                    .as_ref()
                    .map(|nulls| matches!(nulls, syn::NullsOrder::First)),
                using: None,
            });
        }
        Ok(order)
    }
}

impl With {
    fn sqlite(value: Option<&syn::With>) -> Result<Self, AnalysisError> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let mut entries = Vec::new();
        for entry in value.ctes() {
            let mut columns = Vec::new();
            for column in entry.columns() {
                columns.push(crate::sem::unresolved::CteColumn::sqlite(column));
            }
            entries.push(Cte {
                name: entry.table_name().value().to_owned(),
                columns,
                materialized: match entry.materialized() {
                    syn::Materialized::Yes => Some(true),
                    syn::Materialized::No => Some(false),
                    syn::Materialized::Any => None,
                },
                body: CteBody::sqlite(entry.select())?,
                search: None,
                cycle: None,
            });
        }
        Ok(Self {
            recursive: *value.recursive(),
            entries,
        })
    }
}

impl CteBody {
    fn sqlite(query: &syn::Select) -> Result<Self, AnalysisError> {
        Ok(Self::Query(Box::new(Query::sqlite(query)?)))
    }
}

impl crate::sem::unresolved::CteColumn {
    fn sqlite(value: &syn::IndexedColumn) -> Self {
        Self {
            name: value.column_name().value().to_owned(),
            collation: value
                .collation_name()
                .as_ref()
                .map(|name| name.value().to_owned()),
            descending: value
                .order()
                .as_ref()
                .map(|order| matches!(order, syn::SortOrder::Desc)),
        }
    }
}

impl FromItem {
    fn sqlite_from(value: &syn::FromClause) -> Result<Vec<Self>, AnalysisError> {
        let mut left = Self::sqlite(value.select())?;
        for join in value.joins() {
            let (kind, natural) = JoinKind::sqlite(join.operator());
            let (condition, using) = match join.constraint() {
                Some(syn::JoinConstraint::On(expression)) => {
                    (Some(Expr::sqlite(expression)?), Vec::new())
                }
                Some(syn::JoinConstraint::Using(columns)) => {
                    let mut names = Vec::new();
                    for column in columns {
                        names.push(column.value().to_owned());
                    }
                    (None, names)
                }
                None => (None, Vec::new()),
            };
            left = Self::Join(Box::new(Join {
                left,
                right: Self::sqlite(join.table())?,
                kind,
                natural,
                condition,
                using,
                using_alias: None,
                alias: None,
            }));
        }
        Ok(vec![left])
    }

    fn sqlite(value: &syn::SelectTable) -> Result<Self, AnalysisError> {
        Ok(match value {
            syn::SelectTable::Table(value) => {
                let (indexed, not_indexed) = match value.indexed() {
                    Some(syn::Indexed::IndexedBy(name)) => (Some(name.value().to_owned()), false),
                    Some(syn::Indexed::NotIndexed) => (None, true),
                    None => (None, false),
                };
                Self::Relation(Relation {
                    name: Name::sqlite(value.qualified_name()),
                    alias: Alias::sqlite(value.as_()),
                    inherit: false,
                    persistence: String::new(),
                    indexed,
                    not_indexed,
                })
            }
            syn::SelectTable::Select(value) => Self::Query(Box::new(DerivedTable {
                query: Query::sqlite(value.select())?,
                alias: Alias::sqlite(value.as_()),
                lateral: false,
            })),
            syn::SelectTable::Sub(value) => Self::Group(Box::new(FromGroup {
                items: Self::sqlite_from(value.from_clause())?,
                alias: Alias::sqlite(value.as_()),
            })),
            syn::SelectTable::TableCell(value) => Self::Functions(TableFunctions {
                calls: vec![TableFunction {
                    expression: Expr::table_call(
                        &Name::sqlite(value.qualified_name()),
                        &Expr::sqlite_list(value.exprs())?,
                    ),
                    columns: Vec::new(),
                }],
                alias: Alias::sqlite(value.as_()),
                lateral: false,
                ordinality: false,
                rows_from: false,
                columns: Vec::new(),
            }),
        })
    }
}

impl JoinKind {
    fn sqlite(value: &syn::JoinOperator) -> (Self, bool) {
        match value {
            syn::JoinOperator::Comma => (Self::Cross, false),
            syn::JoinOperator::TypedJoin(None) => (Self::Inner, false),
            syn::JoinOperator::TypedJoin(Some(kind)) => {
                let join = if kind.is_left() && kind.is_right() {
                    Self::Full
                } else if kind.is_left() {
                    Self::Left
                } else if kind.is_right() {
                    Self::Right
                } else if kind.is_cross() {
                    Self::Cross
                } else {
                    Self::Inner
                };
                (join, kind.is_natural())
            }
        }
    }
}

impl Window {
    pub(super) fn sqlite_over(value: &syn::Over) -> Result<Self, AnalysisError> {
        match value {
            syn::Over::Window(window) => Self::sqlite(window),
            syn::Over::Name(name) => Ok(Self {
                name: None,
                reference: Some(name.value().to_owned()),
                partition: Vec::new(),
                order: Vec::new(),
                frame: None,
            }),
        }
    }

    fn sqlite(value: &syn::Window) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: None,
            reference: value.base().as_ref().map(|name| name.value().to_owned()),
            partition: Expr::sqlite_list(value.partition_by())?,
            order: Order::sqlite_list(value.order_by())?,
            frame: value
                .frame_clause()
                .as_ref()
                .map(Frame::sqlite)
                .transpose()?,
        })
    }
}

impl Frame {
    fn sqlite(value: &syn::FrameClause) -> Result<Self, AnalysisError> {
        Ok(Self {
            mode: FrameMode::sqlite(value.mode()),
            start: FrameBound::sqlite(value.start())?,
            end: value.end().as_ref().map(FrameBound::sqlite).transpose()?,
            exclusion: value.exclude().as_ref().map(FrameExclusion::sqlite),
        })
    }
}

impl FrameMode {
    const fn sqlite(value: &syn::FrameMode) -> Self {
        match value {
            syn::FrameMode::Rows => Self::Rows,
            syn::FrameMode::Range => Self::Range,
            syn::FrameMode::Groups => Self::Groups,
        }
    }
}

impl FrameBound {
    fn sqlite(value: &syn::FrameBound) -> Result<Self, AnalysisError> {
        Ok(match value {
            syn::FrameBound::CurrentRow => Self::CurrentRow,
            syn::FrameBound::UnboundedPreceding => Self::UnboundedPreceding,
            syn::FrameBound::UnboundedFollowing => Self::UnboundedFollowing,
            syn::FrameBound::Preceding(expr) => Self::Preceding(Expr::sqlite(expr)?),
            syn::FrameBound::Following(expr) => Self::Following(Expr::sqlite(expr)?),
        })
    }
}

impl FrameExclusion {
    const fn sqlite(value: &syn::FrameExclude) -> Self {
        match value {
            syn::FrameExclude::NoOthers => Self::NoOthers,
            syn::FrameExclude::CurrentRow => Self::CurrentRow,
            syn::FrameExclude::Group => Self::Group,
            syn::FrameExclude::Ties => Self::Ties,
        }
    }
}
