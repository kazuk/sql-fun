use crate::syn::sqlite::{Select, SubqueryType, TableInternalId};

use super::Expr;

impl Expr {
    /// create Subquery variant
    pub fn subquery<SEL>(select: SEL) -> Self
    where
        SEL: Into<Select>,
    {
        Self::Subquery(select.into())
    }
}

/// sub query result expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct SubqueryResult {
    subquery_id: TableInternalId,
    lhs: Option<Box<Expr>>,
    not_in: bool,
    query_type: SubqueryType,
}

impl SubqueryResult {
    /// create instance
    pub fn new(
        subquery_id: TableInternalId,
        lhs: Option<Expr>,
        not_in: bool,
        query_type: SubqueryType,
    ) -> Self {
        Self {
            subquery_id,
            lhs: lhs.map(Box::new),
            not_in,
            query_type,
        }
    }
}

impl Expr {
    /// create SubqueryResult variant
    pub fn subquery_result<ID, EX, ST>(
        subquery_id: ID,
        lhs: Option<EX>,
        not_in: bool,
        query_type: ST,
    ) -> Self
    where
        ID: Into<TableInternalId>,
        EX: Into<Self>,
        ST: Into<SubqueryType>,
    {
        Self::SubqueryResult(SubqueryResult::new(
            subquery_id.into(),
            lhs.map(|v| v.into()),
            not_in,
            query_type.into(),
        ))
    }
}
