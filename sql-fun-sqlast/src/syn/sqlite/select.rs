use crate::syn::sqlite::{
    As, Expr, FromClause, GroupBy, Name, SortOrder, SortedColumn, WindowDef, With,
};

/// select statement
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct Select {
    with: Option<With>,
    body: SelectBody,
    order_by: Vec<SortedColumn>,
    limit: Option<Limit>,
}

use turso_parser::ast::Select as SelectIn;

impl From<SelectIn> for Select {
    fn from(value: SelectIn) -> Self {
        Self {
            with: value.with.map(With::from),
            body: SelectBody::from(value.body),
            order_by: value.order_by.into_iter().map(SortedColumn::from).collect(),
            limit: value.limit.map(Limit::from),
        }
    }
}

/// select statement body
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct SelectBody {
    select: OneSelect,
    compounds: Vec<CompoundSelect>,
}

use turso_parser::ast::SelectBody as SelectBodyIn;
impl From<SelectBodyIn> for SelectBody {
    fn from(value: SelectBodyIn) -> Self {
        Self {
            select: OneSelect::from(value.select),
            compounds: value
                .compounds
                .into_iter()
                .map(CompoundSelect::from)
                .collect(),
        }
    }
}

/// one select item
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct OneSelectItem {
    distinctness: Option<Distinctness>,
    columns: Vec<ResultColumn>,
    from: Option<FromClause>,
    where_clause: Option<Box<Expr>>,
    group_by: Option<GroupBy>,
    window_clause: Vec<WindowDef>,
}

impl OneSelectItem {
    /// create instance
    pub fn new(
        distinctness: Option<Distinctness>,
        columns: Vec<ResultColumn>,
        from: Option<FromClause>,
        where_clause: Option<Expr>,
        group_by: Option<GroupBy>,
        window_clause: Vec<WindowDef>,
    ) -> Self {
        Self {
            distinctness,
            columns,
            from,
            where_clause: where_clause.map(Box::new),
            group_by,
            window_clause,
        }
    }
}

use turso_parser::ast::OneSelect as OneSelectIn;

/// one select
#[derive(Clone, Debug)]
pub enum OneSelect {
    /// select
    Select(OneSelectItem),
    /// values
    Values(Vec<Vec<Box<Expr>>>),
}

impl From<OneSelectIn> for OneSelect {
    fn from(value: OneSelectIn) -> Self {
        match value {
            OneSelectIn::Select {
                distinctness,
                columns,
                from,
                where_clause,
                group_by,
                window_clause,
            } => Self::Select(OneSelectItem::new(
                distinctness.map(Distinctness::from),
                columns.into_iter().map(ResultColumn::from).collect(),
                from.map(FromClause::from),
                where_clause.map(|e| Expr::from(*e)),
                group_by.map(GroupBy::from),
                window_clause.into_iter().map(WindowDef::from).collect(),
            )),
            OneSelectIn::Values(items) => Self::Values(
                items
                    .into_iter()
                    .map(|r| r.into_iter().map(|v| Box::new(Expr::from(*v))).collect())
                    .collect(),
            ),
        }
    }
}

use turso_parser::ast::CompoundSelect as CompoundSelectIn;

/// compound select
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct CompoundSelect {
    operator: CompoundOperator,
    select: OneSelect,
}

impl From<CompoundSelectIn> for CompoundSelect {
    fn from(value: CompoundSelectIn) -> Self {
        Self {
            operator: CompoundOperator::from(value.operator),
            select: OneSelect::from(value.select),
        }
    }
}

use turso_parser::ast::CompoundOperator as CompoundOperatorIn;

/// compound operator
#[derive(Clone, Debug)]
pub enum CompoundOperator {
    /// union
    Union,
    /// union all
    UnionAll,
    /// except
    Except,
    /// intersect
    Intersect,
}

impl From<CompoundOperatorIn> for CompoundOperator {
    fn from(value: CompoundOperatorIn) -> Self {
        match value {
            CompoundOperatorIn::Union => Self::Union,
            CompoundOperatorIn::UnionAll => Self::UnionAll,
            CompoundOperatorIn::Except => Self::Except,
            CompoundOperatorIn::Intersect => Self::Intersect,
        }
    }
}

use turso_parser::ast::IndexedColumn as IndexedColumnIn;

/// indexed column
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct IndexedColumn {
    column_name: Name,
    collation_name: Option<Name>,
    order: Option<SortOrder>,
}

impl From<IndexedColumnIn> for IndexedColumn {
    fn from(value: IndexedColumnIn) -> Self {
        Self {
            column_name: Name::from(value.col_name),
            collation_name: value.collation_name.map(Name::from),
            order: value.order.map(SortOrder::from),
        }
    }
}

use turso_parser::ast::Limit as LimitIn;

/// limit clause
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct Limit {
    expr: Box<Expr>,
    offset: Option<Box<Expr>>,
}

impl Limit {
    /// create instance
    pub fn new(expr: Expr, offset: Option<Expr>) -> Self {
        Self {
            expr: Box::new(expr),
            offset: offset.map(Box::new),
        }
    }
}

impl From<LimitIn> for Limit {
    fn from(value: LimitIn) -> Self {
        Self::new(
            Expr::from(*value.expr),
            value.offset.map(|v| Expr::from(*v)),
        )
    }
}

use turso_parser::ast::Distinctness as DistinctnessIn;

/// distinctness
#[derive(Clone, Debug)]
pub enum Distinctness {
    /// distinct
    Distinct,
    /// all
    All,
}

impl From<DistinctnessIn> for Distinctness {
    fn from(value: DistinctnessIn) -> Self {
        match value {
            DistinctnessIn::Distinct => Self::Distinct,
            DistinctnessIn::All => Self::All,
        }
    }
}

use turso_parser::ast::ResultColumn as ResultColumnIn;

/// result column
#[derive(Clone, Debug)]
pub enum ResultColumn {
    /// expressions
    Expr(ResultColumnExpr),
    /// star
    Star,
    /// table with star
    TableStar(Name),
}

impl From<ResultColumnIn> for ResultColumn {
    fn from(value: ResultColumnIn) -> Self {
        match value {
            ResultColumnIn::Expr(expr, as_) => {
                Self::Expr(ResultColumnExpr::new(Expr::from(*expr), as_.map(As::from)))
            }
            ResultColumnIn::Star => Self::Star,
            ResultColumnIn::TableStar(name) => Self::TableStar(Name::from(name)),
        }
    }
}

/// result column expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ResultColumnExpr {
    expr: Box<Expr>,
    as_: Option<As>,
}

impl ResultColumnExpr {
    /// create instance
    pub fn new(expr: Expr, as_: Option<As>) -> Self {
        Self {
            expr: Box::new(expr),
            as_,
        }
    }
}
