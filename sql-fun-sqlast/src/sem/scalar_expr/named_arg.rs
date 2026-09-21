use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference},
    syn::ScanToken,
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
        _scalar_context: &ScalarAnalysisContext<'_>,
        _syn: crate::syn::NamedArgExpr,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }
}
