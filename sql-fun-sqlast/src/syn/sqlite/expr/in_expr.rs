use crate::syn::sqlite::{QualifiedName, Select};

use super::Expr;

/// in list expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprInList {
    lhs: Box<Expr>,
    not: bool,
    rhs: Vec<Expr>,
}

impl ExprInList {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
        }
    }
}

impl Expr {
    /// create InList variant
    pub fn in_list<VALUE, ListItem>(value: VALUE, not: bool, list: Vec<ListItem>) -> Self
    where
        VALUE: Into<Self>,
        ListItem: Into<Self>,
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
        VALUE: Into<Self>,
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
    args: Vec<Expr>,
}

impl ExprInTable {
    /// create instance
    pub fn new(lhs: Expr, not: bool, rhs: QualifiedName, args: Vec<Expr>) -> Self {
        Self {
            lhs: Box::new(lhs),
            not,
            rhs,
            args,
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
        VALUE: Into<Self>,
        TABLE: Into<QualifiedName>,
        ARG: Into<Self>,
    {
        Self::InTable(ExprInTable::new(
            value.into(),
            not,
            table.into(),
            args.into_iter().map(|v| v.into()).collect(),
        ))
    }
}
