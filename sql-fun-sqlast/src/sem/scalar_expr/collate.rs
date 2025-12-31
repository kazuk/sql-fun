use crate::sem::{
    AnalysisError, FromClause, ParseContext, SemScalarExpr, TypeReference, WithClause,
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

/// Collate expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollateExpr {}

impl SemScalarExprNode for CollateExpr {
    fn get_type(&self) -> Option<TypeReference> {
        todo!()
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::CollateExpr> for CollateExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        _context: TParseContext,
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        _syn: crate::syn::CollateExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }
}
