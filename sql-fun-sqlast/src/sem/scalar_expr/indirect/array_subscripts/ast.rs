use crate::{
    ast::{Access, Expr, Slice},
    context::ParseContext,
    sem::{AnalysisError, ArraySubscript, ScalarAnalysisContext, SemScalarExpr},
};

impl ArraySubscript {
    pub(crate) fn from_ast_access(
        ast: &Access,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Access::Index(index) => Self::from_ast_index(index, context, scope),
            Access::Slice(slice) => Self::from_ast_slice(slice, context, scope),
            _ => AnalysisError::raise_unexpected_input("array subscript"),
        }
    }
}

impl ArraySubscript {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let index = SemScalarExpr::from_ast_in(index, context, scope)?;
        Ok(Self::index(&index))
    }

    pub(crate) fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let lower = SemScalarExpr::from_ast_optional(&slice.lower, context, scope)?;
        let upper = SemScalarExpr::from_ast_optional(&slice.upper, context, scope)?;
        Ok(Self::slice(lower.as_ref(), upper.as_ref()))
    }
}
