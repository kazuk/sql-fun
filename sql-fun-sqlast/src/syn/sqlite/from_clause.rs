use crate::syn::sqlite::{As, Expr, Name, QualifiedName, Select};

use turso_parser::ast::FromClause as FromClauseIn;

/// from clause
#[derive(Debug, derive_getters::Getters)]
pub struct FromClause {
    select: Box<SelectTable>,
    joins: Vec<JoinedSelectTable>,
}

impl From<FromClauseIn> for FromClause {
    fn from(value: FromClauseIn) -> Self {
        Self {
            select: Box::new(SelectTable::from(*value.select)),
            joins: value
                .joins
                .into_iter()
                .map(JoinedSelectTable::from)
                .collect(),
        }
    }
}

/// select table tbale
#[derive(Debug, derive_getters::Getters)]
pub struct SelectTableTable {
    qualified_name: QualifiedName,
    as_: Option<As>,
    indexed: Option<Indexed>,
}

impl SelectTableTable {
    /// create instance
    pub fn new(qualified_name: QualifiedName, as_: Option<As>, indexed: Option<Indexed>) -> Self {
        Self {
            qualified_name,
            as_,
            indexed,
        }
    }
}

/// select table cell
#[derive(Debug, derive_getters::Getters)]
pub struct SelectTableTableCell {
    qualified_name: QualifiedName,
    exprs: Vec<Box<Expr>>,
    as_: Option<As>,
}

impl SelectTableTableCell {
    /// create instance
    pub fn new(qualified_name: QualifiedName, exprs: Vec<Expr>, as_: Option<As>) -> Self {
        Self {
            qualified_name,
            exprs: exprs.into_iter().map(Box::new).collect(),
            as_,
        }
    }
}

/// select table select
#[derive(Debug, derive_getters::Getters)]
pub struct SelectTableSelect {
    select: Select,
    as_: Option<As>,
}

impl SelectTableSelect {
    /// create instance
    pub fn new(select: Select, as_: Option<As>) -> Self {
        Self { select, as_ }
    }
}

/// select table sub
#[derive(Debug, derive_getters::Getters)]
pub struct SelectTableSub {
    from_clause: FromClause,
    as_: Option<As>,
}

impl SelectTableSub {
    /// create instance
    pub fn new(from_clause: FromClause, as_: Option<As>) -> Self {
        Self { from_clause, as_ }
    }
}

use turso_parser::ast::SelectTable as SelectTableIn;

/// select table
#[derive(Debug)]
pub enum SelectTable {
    /// table
    Table(SelectTableTable),
    /// table cell
    TableCell(SelectTableTableCell),
    /// select
    Select(SelectTableSelect),
    /// sub
    Sub(SelectTableSub),
}

impl From<SelectTableIn> for SelectTable {
    fn from(value: SelectTableIn) -> Self {
        match value {
            SelectTableIn::Table(qualified_name, as_, indexed) => {
                Self::Table(SelectTableTable::new(
                    QualifiedName::from(qualified_name),
                    as_.map(As::from),
                    indexed.map(Indexed::from),
                ))
            }
            SelectTableIn::TableCall(qualified_name, exprs, as_) => {
                Self::TableCell(SelectTableTableCell::new(
                    QualifiedName::from(qualified_name),
                    exprs.into_iter().map(|e| Expr::from(*e)).collect(),
                    as_.map(As::from),
                ))
            }
            SelectTableIn::Select(select, as_) => Self::Select(SelectTableSelect::new(
                Select::from(select),
                as_.map(As::from),
            )),
            SelectTableIn::Sub(from_clause, as_) => Self::Sub(SelectTableSub::new(
                FromClause::from(from_clause),
                as_.map(As::from),
            )),
        }
    }
}

use turso_parser::ast::JoinedSelectTable as JoinedSelectTableIn;

/// joined select table
#[derive(Debug, derive_getters::Getters)]
pub struct JoinedSelectTable {
    operator: JoinOperator,
    table: Box<SelectTable>,
    constraint: Option<JoinConstraint>,
}

impl From<JoinedSelectTableIn> for JoinedSelectTable {
    fn from(value: JoinedSelectTableIn) -> Self {
        Self {
            operator: JoinOperator::from(value.operator),
            table: Box::new(SelectTable::from(*value.table)),
            constraint: value.constraint.map(JoinConstraint::from),
        }
    }
}

use turso_parser::ast::Indexed as IndexedIn;

/// indexed
#[derive(Debug)]
pub enum Indexed {
    /// indexed by
    IndexedBy(Name),
    /// not indexed
    NotIndexed,
}

impl From<IndexedIn> for Indexed {
    fn from(value: IndexedIn) -> Self {
        match value {
            IndexedIn::IndexedBy(name) => Self::IndexedBy(Name::from(name)),
            IndexedIn::NotIndexed => Self::NotIndexed,
        }
    }
}

use turso_parser::ast::JoinOperator as JoinOperatorIn;

/// join operator
#[derive(Debug)]
pub enum JoinOperator {
    /// comma
    Comma,
    /// typed join
    TypedJoin(Option<JoinType>),
}

impl From<JoinOperatorIn> for JoinOperator {
    fn from(value: JoinOperatorIn) -> Self {
        match value {
            JoinOperatorIn::Comma => Self::Comma,
            JoinOperatorIn::TypedJoin(join_type) => Self::TypedJoin(join_type.map(JoinType::from)),
        }
    }
}

use turso_parser::ast::JoinConstraint as JoinConstraintIn;

/// join constraint
#[derive(Debug)]
pub enum JoinConstraint {
    /// on
    On(Box<Expr>),
    /// using
    Using(Vec<Name>),
}

impl From<JoinConstraintIn> for JoinConstraint {
    fn from(value: JoinConstraintIn) -> Self {
        match value {
            JoinConstraintIn::On(expr) => Self::On(Box::new(Expr::from(*expr))),
            JoinConstraintIn::Using(names) => {
                Self::Using(names.into_iter().map(Name::from).collect())
            }
        }
    }
}

use turso_parser::ast::JoinType as JoinTypeIn;

/// join type
#[derive(Debug)]
pub struct JoinType(JoinTypeIn);

impl JoinType {
    /// inner join
    pub fn is_inner(&self) -> bool {
        (self.0 & JoinTypeIn::INNER) == JoinTypeIn::INNER
    }

    /// cross join
    pub fn is_cross(&self) -> bool {
        (self.0 & JoinTypeIn::CROSS) == JoinTypeIn::CROSS
    }

    /// natural join
    pub fn is_natural(&self) -> bool {
        (self.0 & JoinTypeIn::NATURAL) == JoinTypeIn::NATURAL
    }

    /// left join
    pub fn is_left(&self) -> bool {
        (self.0 & JoinTypeIn::LEFT) == JoinTypeIn::LEFT
    }

    /// right join
    pub fn is_right(&self) -> bool {
        (self.0 & JoinTypeIn::RIGHT) == JoinTypeIn::RIGHT
    }

    /// outer join
    pub fn is_outer(&self) -> bool {
        (self.0 & JoinTypeIn::OUTER) == JoinTypeIn::OUTER
    }
}

impl From<JoinTypeIn> for JoinType {
    fn from(value: JoinTypeIn) -> Self {
        Self(value)
    }
}
