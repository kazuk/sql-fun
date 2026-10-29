use crate::ast::{
    Alias, Call, Cte, CteBody, CteColumn, DerivedTable, Expr, Frame, FrameBound, FrameExclusion,
    FrameMode, FromGroup, FromItem, FromList, FromOpt, Grouping, Join, JoinKind, Name, Order,
    Query, QueryBody, Relation, Select, SetOperation, SetQuery, TableFunction, TableFunctions,
    Target, TryFromList, TryFromOpt, Window, With,
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

impl TryFrom<&syn::Select> for Query {
    type Error = AnalysisError;

    fn try_from(value: &syn::Select) -> Result<Self, Self::Error> {
        crate::sem::with_stack(|| Self::convert_sqlite(value))
    }
}

impl Query {
    #[inline(never)]
    fn convert_sqlite(value: &syn::Select) -> Result<Self, AnalysisError> {
        let mut query = Self::from(QueryBody::try_from(value.body().select())?);
        for compound in value.body().compounds() {
            let right = Self::from(QueryBody::try_from(compound.select())?);
            query = Self::from(QueryBody::compound(compound.operator(), query, right));
        }
        query.with = With::try_from_opt(value.with().as_ref())?.unwrap_or_default();
        query.order = Order::try_from_list(value.order_by())?;
        if let Some(limit) = value.limit() {
            query.limit = Some(Expr::try_from(limit.expr())?);
            query.offset = Expr::try_from_opt(limit.offset().as_deref())?;
        }
        Ok(query)
    }
}

impl TryFrom<&Box<syn::Select>> for Query {
    type Error = AnalysisError;

    fn try_from(value: &Box<syn::Select>) -> Result<Self, Self::Error> {
        Self::try_from(value.as_ref())
    }
}

impl TryFrom<&syn::OneSelectItem> for Select {
    type Error = AnalysisError;

    fn try_from(value: &syn::OneSelectItem) -> Result<Self, Self::Error> {
        let (group, having) = if let Some(group) = value.group_by() {
            (
                Grouping::try_from_list(group.exprs())?,
                Expr::try_from_opt(group.having().as_deref())?,
            )
        } else {
            (Vec::new(), None)
        };
        Ok(Self {
            into: None,
            distinct: if matches!(value.distinctness(), Some(syn::Distinctness::Distinct)) {
                Some(Vec::new())
            } else {
                None
            },
            targets: Target::try_from_list(value.columns())?,
            from: FromItem::try_from_clause_opt(value.from().as_ref())?,
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
            group,
            group_distinct: false,
            having,
            windows: Window::try_from_list(value.window_clause())?,
        })
    }
}

impl SetQuery {
    fn new_compound(operator: &syn::CompoundOperator, left: Query, right: Query) -> Self {
        Self {
            operation: SetOperation::from(operator),
            all: matches!(operator, syn::CompoundOperator::UnionAll),
            left,
            right,
        }
    }
}

impl QueryBody {
    fn compound(operator: &syn::CompoundOperator, left: Query, right: Query) -> Self {
        Self::Set(Box::new(SetQuery::new_compound(operator, left, right)))
    }
}

impl TryFrom<&Vec<Vec<Box<syn::Expr>>>> for QueryBody {
    type Error = AnalysisError;

    fn try_from(rows: &Vec<Vec<Box<syn::Expr>>>) -> Result<Self, Self::Error> {
        let mut values = Vec::new();
        for row in rows {
            let mut expressions = Vec::new();
            for expression in row {
                expressions.push(Expr::try_from(expression)?);
            }
            values.push(expressions);
        }
        Ok(Self::Values(values))
    }
}

impl TryFrom<&syn::OneSelectItem> for QueryBody {
    type Error = AnalysisError;

    fn try_from(value: &syn::OneSelectItem) -> Result<Self, Self::Error> {
        Ok(Self::Select(Box::new(Select::try_from(value)?)))
    }
}

impl TryFrom<&syn::OneSelect> for QueryBody {
    type Error = AnalysisError;

    fn try_from(value: &syn::OneSelect) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::OneSelect::Values(rows) => Self::try_from(rows)?,
            syn::OneSelect::Select(value) => Self::try_from(value)?,
        })
    }
}

impl From<&syn::CompoundOperator> for SetOperation {
    fn from(value: &syn::CompoundOperator) -> Self {
        match value {
            syn::CompoundOperator::Union | syn::CompoundOperator::UnionAll => Self::Union,
            syn::CompoundOperator::Except => Self::Except,
            syn::CompoundOperator::Intersect => Self::Intersect,
        }
    }
}

impl TryFrom<&syn::ResultColumnExpr> for Target {
    type Error = AnalysisError;

    fn try_from(value: &syn::ResultColumnExpr) -> Result<Self, Self::Error> {
        Ok(Self {
            implicit_alias: matches!(value.as_(), Some(syn::As::ImplicitColumnName(_))),
            location: None,
            expression: Expr::try_from(value.expr())?,
            alias: value.as_().as_ref().map(alias_name),
        })
    }
}

impl TryFrom<&syn::ResultColumn> for Target {
    type Error = AnalysisError;

    fn try_from(value: &syn::ResultColumn) -> Result<Self, Self::Error> {
        match value {
            syn::ResultColumn::Expr(value) => Self::try_from(value),
            syn::ResultColumn::Star => Ok(Self::star()),
            syn::ResultColumn::TableStar(name) => Self::try_from(name),
        }
    }
}

impl TryFrom<&syn::Name> for Target {
    type Error = AnalysisError;

    fn try_from(value: &syn::Name) -> Result<Self, Self::Error> {
        Ok(Self {
            implicit_alias: false,
            location: None,
            expression: Expr::star(&Name::local(value.value())),
            alias: None,
        })
    }
}

impl Target {
    fn star() -> Self {
        Self {
            implicit_alias: false,
            location: None,
            expression: Expr::star(&Name::default()),
            alias: None,
        }
    }
}

impl Expr {
    fn star(name: &Name) -> Self {
        Self::Star(name.clone())
    }
    fn table_call(name: &Name, arguments: &[Self]) -> Self {
        let mut call = Call::base("");
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

impl From<&syn::As> for Alias {
    fn from(value: &syn::As) -> Self {
        Self {
            name: alias_name(value),
            columns: Vec::new(),
        }
    }
}

impl TryFrom<&syn::Expr> for Grouping {
    type Error = AnalysisError;

    fn try_from(value: &syn::Expr) -> Result<Self, Self::Error> {
        Ok(Self::Expression(Expr::try_from(value)?))
    }
}

impl TryFrom<&syn::SortedColumn> for Order {
    type Error = AnalysisError;

    fn try_from(value: &syn::SortedColumn) -> Result<Self, Self::Error> {
        Ok(Self {
            expression: Expr::try_from(value.expr())?,
            descending: value
                .order()
                .map(|order| matches!(order, syn::SortOrder::Desc)),
            nulls_first: value
                .nulls()
                .as_ref()
                .map(|nulls| matches!(nulls, syn::NullsOrder::First)),
            using: None,
        })
    }
}

impl TryFrom<&syn::With> for With {
    type Error = AnalysisError;

    fn try_from(value: &syn::With) -> Result<Self, Self::Error> {
        let mut entries = Vec::new();
        for entry in value.ctes() {
            entries.push(Cte {
                name: entry.table_name().value().to_owned(),
                columns: CteColumn::from_list(entry.columns()),
                materialized: match entry.materialized() {
                    syn::Materialized::Yes => Some(true),
                    syn::Materialized::No => Some(false),
                    syn::Materialized::Any => None,
                },
                body: CteBody::try_from(entry.select())?,
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

impl TryFrom<&syn::Select> for CteBody {
    type Error = AnalysisError;

    fn try_from(query: &syn::Select) -> Result<Self, Self::Error> {
        Ok(Self::query(Query::try_from(query)?))
    }
}

impl From<&syn::IndexedColumn> for CteColumn {
    fn from(value: &syn::IndexedColumn) -> Self {
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
    pub(super) fn try_from_clause_opt(
        value: Option<&syn::FromClause>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let Some(value) = value else {
            return Ok(Vec::new());
        };
        Self::from_clause(value)
    }

    fn from_clause(value: &syn::FromClause) -> Result<Vec<Self>, AnalysisError> {
        let mut left = Self::try_from(value.select())?;
        for join in value.joins() {
            let (kind, natural) = JoinKind::from_sqlite(join.operator());
            let (condition, using) = match join.constraint() {
                Some(syn::JoinConstraint::On(expression)) => {
                    (Some(Expr::try_from(expression)?), Vec::new())
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
            left = Self::join(Join {
                left,
                right: Self::try_from(join.table())?,
                kind,
                natural,
                condition,
                using,
                using_alias: None,
                alias: None,
            });
        }
        Ok(vec![left])
    }
}

impl TryFrom<&syn::SelectTable> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTable) -> Result<Self, Self::Error> {
        crate::sem::with_stack(|| Self::convert_sqlite(value))
    }
}

impl FromItem {
    #[inline(never)]
    fn convert_sqlite(value: &syn::SelectTable) -> Result<Self, AnalysisError> {
        match value {
            syn::SelectTable::Table(value) => Self::try_from(value),
            syn::SelectTable::Select(value) => Self::try_from(value),
            syn::SelectTable::Sub(value) => Self::try_from(value),
            syn::SelectTable::TableCell(value) => Self::try_from(value),
        }
    }
}

impl TryFrom<&syn::SelectTableTable> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableTable) -> Result<Self, Self::Error> {
        Ok(Self::relation(Relation::try_from(value)?))
    }
}

impl TryFrom<&syn::SelectTableTable> for Relation {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableTable) -> Result<Self, Self::Error> {
        let (indexed, not_indexed) = match value.indexed() {
            Some(syn::Indexed::IndexedBy(name)) => (Some(name.value().to_owned()), false),
            Some(syn::Indexed::NotIndexed) => (None, true),
            None => (None, false),
        };
        Ok(Self {
            name: Name::from(value.qualified_name()),
            alias: value.as_().as_ref().map(Alias::from),
            inherit: false,
            persistence: String::new(),
            indexed,
            not_indexed,
        })
    }
}

impl TryFrom<&syn::SelectTableSelect> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableSelect) -> Result<Self, Self::Error> {
        Ok(Self::query(DerivedTable::try_from(value)?))
    }
}

impl TryFrom<&syn::SelectTableSelect> for DerivedTable {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableSelect) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from(value.select())?,
            alias: value.as_().as_ref().map(Alias::from),
            lateral: false,
        })
    }
}

impl TryFrom<&syn::SelectTableSub> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableSub) -> Result<Self, Self::Error> {
        Ok(Self::group(FromGroup::try_from(value)?))
    }
}

impl TryFrom<&syn::SelectTableSub> for FromGroup {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableSub) -> Result<Self, Self::Error> {
        Ok(Self {
            items: FromItem::from_clause(value.from_clause())?,
            alias: value.as_().as_ref().map(Alias::from),
        })
    }
}

impl TryFrom<&syn::SelectTableTableCell> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableTableCell) -> Result<Self, Self::Error> {
        Ok(Self::functions(TableFunctions::single(
            TableFunction::try_from(value)?,
            value.as_().as_ref().map(Alias::from),
        )))
    }
}

impl TryFrom<&syn::SelectTableTableCell> for TableFunction {
    type Error = AnalysisError;

    fn try_from(value: &syn::SelectTableTableCell) -> Result<Self, Self::Error> {
        Ok(Self {
            expression: Expr::table_call(
                &Name::from(value.qualified_name()),
                &Expr::try_from_list(value.exprs())?,
            ),
            columns: Vec::new(),
        })
    }
}

impl TryFrom<&Box<syn::SelectTable>> for FromItem {
    type Error = AnalysisError;

    fn try_from(value: &Box<syn::SelectTable>) -> Result<Self, Self::Error> {
        Self::try_from(value.as_ref())
    }
}

impl JoinKind {
    fn from_sqlite(value: &syn::JoinOperator) -> (Self, bool) {
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

impl TryFrom<&syn::Over> for Window {
    type Error = AnalysisError;

    fn try_from(value: &syn::Over) -> Result<Self, Self::Error> {
        match value {
            syn::Over::Window(window) => Self::try_from(window),
            syn::Over::Name(name) => Self::try_from(name),
        }
    }
}

impl TryFrom<&syn::Name> for Window {
    type Error = AnalysisError;

    fn try_from(value: &syn::Name) -> Result<Self, Self::Error> {
        Ok(Self {
            reference_only: true,
            name: None,
            reference: Some(value.value().to_owned()),
            partition: Vec::new(),
            order: Vec::new(),
            frame: None,
        })
    }
}

impl TryFrom<&syn::Window> for Window {
    type Error = AnalysisError;

    fn try_from(value: &syn::Window) -> Result<Self, Self::Error> {
        Ok(Self {
            reference_only: false,
            name: None,
            reference: String::from_opt(value.base().as_ref()),
            partition: Expr::try_from_list(value.partition_by())?,
            order: Order::try_from_list(value.order_by())?,
            frame: Frame::try_from_opt(value.frame_clause().as_ref())?,
        })
    }
}

impl TryFrom<&syn::WindowDef> for Window {
    type Error = AnalysisError;

    fn try_from(value: &syn::WindowDef) -> Result<Self, Self::Error> {
        let mut window = Self::try_from(value.window())?;
        window.name = Some(value.name().value().to_owned());
        Ok(window)
    }
}

impl FromOpt<&syn::Name> for String {
    fn from_opt(value: Option<&syn::Name>) -> Option<Self> {
        value.map(|name| name.value().to_owned())
    }
}

impl TryFrom<&syn::FrameClause> for Frame {
    type Error = AnalysisError;

    fn try_from(value: &syn::FrameClause) -> Result<Self, Self::Error> {
        Ok(Self {
            mode: FrameMode::from(value.mode()),
            start: FrameBound::try_from(value.start())?,
            end: FrameBound::try_from_opt(value.end().as_ref())?,
            exclusion: value.exclude().as_ref().map(FrameExclusion::from),
        })
    }
}

impl From<&syn::FrameMode> for FrameMode {
    fn from(value: &syn::FrameMode) -> Self {
        match value {
            syn::FrameMode::Rows => Self::Rows,
            syn::FrameMode::Range => Self::Range,
            syn::FrameMode::Groups => Self::Groups,
        }
    }
}

impl TryFrom<&syn::FrameBound> for FrameBound {
    type Error = AnalysisError;

    fn try_from(value: &syn::FrameBound) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::FrameBound::CurrentRow => Self::CurrentRow,
            syn::FrameBound::UnboundedPreceding => Self::UnboundedPreceding,
            syn::FrameBound::UnboundedFollowing => Self::UnboundedFollowing,
            syn::FrameBound::Preceding(expr) => Self::Preceding(Expr::try_from(expr)?),
            syn::FrameBound::Following(expr) => Self::Following(Expr::try_from(expr)?),
        })
    }
}

impl From<&syn::FrameExclude> for FrameExclusion {
    fn from(value: &syn::FrameExclude) -> Self {
        match value {
            syn::FrameExclude::NoOthers => Self::NoOthers,
            syn::FrameExclude::CurrentRow => Self::CurrentRow,
            syn::FrameExclude::Group => Self::Group,
            syn::FrameExclude::Ties => Self::Ties,
        }
    }
}
