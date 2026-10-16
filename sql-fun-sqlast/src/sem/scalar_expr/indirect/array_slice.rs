use crate::sem::scalar_expr::indirect::IndirectionExpr;
use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference},
    syn::{Opt, ScanToken},
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySlice {
    /// array
    array: SemScalarExpr,
    /// lower bound
    lower: Option<SemScalarExpr>,
    /// upper bound
    upper: Option<SemScalarExpr>,
}

impl ArraySlice {
    pub const fn new(
        array: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
    ) -> Self {
        Self {
            array,
            lower,
            upper,
        }
    }

    pub fn array_type(&self) -> Option<TypeReference> {
        self.array.get_type()
    }

    pub fn array_is_not_null(&self) -> Option<bool> {
        self.array.is_not_null()
    }

    pub fn array_slice<TParseContext>(
        array: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::slice_resolved(array, lower, upper, context)
    }

    pub(super) fn slice_resolved<TContext: crate::sem::resolution::ResolutionContext>(
        array: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
        context: &mut TContext,
    ) -> Result<IndirectionExpr, AnalysisError> {
        array.require_array_resolved(context)?;
        if let Some(lower_index) = &lower {
            lower_index.fits_array_index_resolved(context)?;
        }
        if let Some(upper_index) = &upper {
            upper_index.fits_array_index_resolved(context)?;
        }

        Ok(IndirectionExpr::ArraySlice(Box::new(Self::new(
            array, lower, upper,
        ))))
    }

    fn analyze_index<TParseContext>(
        context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        index: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Option<SemScalarExpr>, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(index) = index.as_inner() else {
            return Ok((None, context));
        };
        let (lexpr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, scalar_context, index, tokens)?;
        Ok((Some(lexpr), new_context))
    }

    pub fn analyze_slice<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        arg: SemScalarExpr,
        indices: crate::syn::AIndices,
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndirectionExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (lexpr, new_context) =
            Self::analyze_index(context, scalar_context, indices.get_lidx(), tokens)?;
        context = new_context;
        let (rexpr, new_context) =
            Self::analyze_index(context, scalar_context, indices.get_uidx(), tokens)?;
        context = new_context;
        let arg = Self::array_slice(arg, lexpr, rexpr, &mut context)?;
        Ok((arg, context))
    }
}

impl ArraySlice {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.array.walk(visitor)?;
        crate::sem::traversal::optional(&mut self.lower, visitor)?;
        crate::sem::traversal::optional(&mut self.upper, visitor)?;
        Ok(())
    }
}

impl ArraySlice {
    pub(crate) fn requires_type_inference(&self) -> bool {
        self.array.needs_type_inference()
            || self
                .lower
                .as_ref()
                .is_some_and(SemScalarExpr::needs_type_inference)
            || self
                .upper
                .as_ref()
                .is_some_and(SemScalarExpr::needs_type_inference)
    }
}
