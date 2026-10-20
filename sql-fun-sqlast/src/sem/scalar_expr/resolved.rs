use crate::sem::scalar_expr::{
    ArrayExpr, BooleanExpr, ExistsExpr, ListExpr, NullTestExpr, ScalarConstExpr, SemScalarExpr,
    SubLinkExpr, TypeCastExpr,
    boolean::{BooleanExprBody, NullTestType},
};
use crate::sem::{AnalysisError, SelectStatement, TypeReference};

impl SemScalarExpr {
    pub(crate) fn function_call(value: &crate::sem::FuncCallExpr) -> Self {
        Self::FuncCall(value.clone())
    }
    pub(crate) fn sql_value(value: &crate::sem::SqlValueExpr) -> Self {
        Self::SqlValue(value.clone())
    }
    pub(crate) fn indirection(value: &crate::sem::IndirectionExpr) -> Self {
        Self::Indirection(value.clone())
    }
    pub(crate) fn coalesce(value: &super::CoalesceExpr) -> Self {
        Self::Coalesce(value.clone())
    }
    pub(crate) fn case(value: &super::CaseExpr) -> Self {
        Self::Case(value.clone())
    }
    pub(crate) fn into_list_values(self) -> Result<Vec<Self>, AnalysisError> {
        match self {
            Self::List(values) => Ok(values.into_inner()),
            _ => AnalysisError::raise_unexpected_input("expression list"),
        }
    }

    pub(crate) fn literal(value: &ScalarConstExpr) -> Self {
        Self::Const(value.clone())
    }
    pub(crate) fn analyzed_array(value: &ArrayExpr) -> Self {
        Self::Array(value.clone())
    }
    pub(crate) fn list(values: &[Self]) -> Self {
        Self::List(ListExpr(values.to_vec()))
    }
    pub(crate) fn logical_and(values: &[Self]) -> Self {
        Self::Boolean(BooleanExpr::new(BooleanExprBody::and(values)))
    }
    pub(crate) fn logical_or(values: &[Self]) -> Self {
        Self::Boolean(BooleanExpr::new(BooleanExprBody::or(values)))
    }
    pub(crate) fn logical_not(value: &Self) -> Self {
        Self::Boolean(BooleanExpr::new(BooleanExprBody::not(value)))
    }
    pub(crate) fn null_test(value: &Self, negated: bool) -> Self {
        let kind = NullTestType::from_negated(negated);
        Self::NullTest(NullTestExpr::new(kind, value.clone()))
    }
    pub(crate) fn explicit_cast(value: &Self, target: &TypeReference) -> Self {
        Self::TypeCast(TypeCastExpr::from_resolved(target, value))
    }
    pub(crate) fn exists(query: &SelectStatement) -> Self {
        Self::Exists(ExistsExpr::from(query))
    }
    /// EXISTS predicate, when this expression directly represents one.
    pub const fn as_exists(&self) -> Option<&ExistsExpr> {
        match self {
            Self::Exists(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn subquery(query: &SelectStatement) -> Result<Self, AnalysisError> {
        Ok(Self::SubLink(SubLinkExpr::from_resolved(query)?))
    }
}
