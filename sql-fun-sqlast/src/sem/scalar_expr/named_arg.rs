use crate::sem::{
    AnalysisError, FromClause, ParseContext, SemScalarExpr, TypeReference, WithClause,
};

use super::SemScalarExprNode;

/// named argument expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NamedArgExpr {}

impl SemScalarExprNode for NamedArgExpr {
    fn get_type(&self) -> Option<TypeReference> {
        todo!()
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}

impl<TParseContext> super::AnalyzeScalarExpr<TParseContext, crate::syn::NamedArgExpr>
    for NamedArgExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        _context: TParseContext,
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        _syn: crate::syn::NamedArgExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }
}
