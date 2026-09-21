use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, TypeReference},
    syn::ScanToken,
};

use super::{AnalyzeScalarExpr, SemScalarExpr, SemScalarExprNode};

/// parameter reference expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamRef {}

impl SemScalarExprNode for ParamRef {
    fn get_type(&self) -> Option<TypeReference> {
        todo!()
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::ParamRef> for ParamRef
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        _context: TParseContext,
        _scalar_context: &ScalarAnalysisContext<'_>,
        _syn: crate::syn::ParamRef,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }
}
