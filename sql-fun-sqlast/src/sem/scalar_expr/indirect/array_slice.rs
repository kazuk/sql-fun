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

    /// Array expression being sliced.
    pub const fn array(&self) -> &SemScalarExpr {
        &self.array
    }

    /// Explicit lower bound; absence preserves PostgreSQL's omitted-bound form.
    pub const fn lower(&self) -> Option<&SemScalarExpr> {
        self.lower.as_ref()
    }

    /// Explicit upper bound; absence preserves PostgreSQL's omitted-bound form.
    pub const fn upper(&self) -> Option<&SemScalarExpr> {
        self.upper.as_ref()
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

    pub(super) fn slice_resolved<TContext: crate::sem::ParseContext>(
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
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.array.walk(visitor)?;
        crate::sem::traversal::optional(&mut self.lower, visitor)?;
        crate::sem::traversal::optional(&mut self.upper, visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.array.visit(visitor)?;
        crate::sem::traversal::visit_optional(&self.lower, visitor)?;
        crate::sem::traversal::visit_optional(&self.upper, visitor)?;
        Ok(())
    }
}

impl ArraySlice {
    pub(super) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.walk_assignment_target(visitor)?;
        for index in [&mut self.lower, &mut self.upper].into_iter().flatten() {
            index.walk(visitor)?;
            visitor.subscript(index);
        }
        Ok(())
    }
    pub(super) fn visit_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.visit_assignment_target(visitor)?;
        for index in [&self.lower, &self.upper].into_iter().flatten() {
            index.visit(visitor)?;
            visitor.subscript(index);
        }
        Ok(())
    }
}

impl ArraySlice {
    pub(super) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl crate::sem::ParseContext,
    ) -> SemScalarExpr {
        let bounds: Vec<_> = self.lower.iter().chain(&self.upper).collect();
        let mut result = parameters.array_access(context, &self.array, &bounds, true);
        result.retain_indirection(&IndirectionExpr::slice(self));
        result
    }
}
