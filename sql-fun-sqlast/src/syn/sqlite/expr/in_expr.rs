use crate::syn::sqlite::{QualifiedName, Select};

use super::Expr;

/// in list expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprInList {
    lhs: Box<Expr>,
    not: bool,
    rhs: Vec<Box<Expr>>,
}

impl ExprInList {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs: rhs.into_iter().map(Box::new).collect(),
        }
    }
}

impl Expr {
    /// create InList variant
    pub fn in_list<VALUE, ListItem>(value: VALUE, not: bool, list: Vec<ListItem>) -> Self
    where
        VALUE: Into<Expr>,
        ListItem: Into<Expr>,
    {
        Self::InList(ExprInList::new(
            value.into(),
            not,
            list.into_iter().map(|v| v.into()).collect(),
        ))
    }
}

/// in select expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprInSelect {
    lhs: Box<Expr>,
    not: bool,
    rhs: Select,
}

impl ExprInSelect {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: Select) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
        }
    }
}

impl Expr {
    /// create InSelect variant
    pub fn in_select<VALUE, SELECT>(value: VALUE, not: bool, select: SELECT) -> Self
    where
        VALUE: Into<Expr>,
        SELECT: Into<Select>,
    {
        Self::InSelect(ExprInSelect::new(value.into(), not, select.into()))
    }
}

/// in table expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprInTable {
    lhs: Box<Expr>,
    not: bool,
    rhs: QualifiedName,
    args: Vec<Box<Expr>>,
}

impl ExprInTable {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: QualifiedName, args: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
            args: args.into_iter().map(Box::new).collect(),
        }
    }
}

impl Expr {
    /// create InTable variant
    pub fn in_table<VALUE, TABLE, ARG>(
        value: VALUE,
        not: bool,
        table: TABLE,
        args: Vec<ARG>,
    ) -> Self
    where
        VALUE: Into<Expr>,
        TABLE: Into<QualifiedName>,
        ARG: Into<Expr>,
    {
        Self::InTable(ExprInTable::new(
            value.into(),
            not,
            table.into(),
            args.into_iter().map(|v| v.into()).collect(),
        ))
    }
}
