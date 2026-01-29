use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, FromClause, ParseContext, TypeReference, WithClause},
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
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        _syn: crate::syn::ParamRef,
        _tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        todo!()
    }
}
