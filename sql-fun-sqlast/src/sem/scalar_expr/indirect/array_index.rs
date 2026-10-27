use crate::sem::scalar_expr::indirect::IndirectionExpr;
use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference},
    syn::{Opt, ScanToken},
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayIndex {
    array: SemScalarExpr,
    index: SemScalarExpr,
    item_type: Option<TypeReference>,
}

impl ArrayIndex {
    pub const fn new(
        array: SemScalarExpr,
        index: SemScalarExpr,
        item_type: Option<TypeReference>,
    ) -> Self {
        Self {
            array,
            index,
            item_type,
        }
    }

    pub const fn array(&self) -> &SemScalarExpr {
        &self.array
    }

    pub(super) fn set_item_type(&mut self, item_type: &Option<TypeReference>) {
        self.item_type = item_type.clone();
    }

    /// Index expression, including its inferred parameter domain.
    pub const fn index(&self) -> &SemScalarExpr {
        &self.index
    }

    pub const fn item_type(&self) -> &Option<TypeReference> {
        &self.item_type
    }

    pub fn array_index<TParseContext>(
        array: SemScalarExpr,
        index: SemScalarExpr,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::index_resolved(array, index, context)
    }

    pub(super) fn index_resolved<TContext: crate::sem::ParseContext>(
        array: SemScalarExpr,
        index: SemScalarExpr,
        context: &mut TContext,
    ) -> Result<IndirectionExpr, AnalysisError> {
        array.require_array_resolved(context)?;
        index.fits_array_index_resolved(context)?;
        let item_type = if let Some(array_type) = array.get_type() {
            Self::resolved_item_type(context, &array_type)
        } else {
            None
        };

        Ok(IndirectionExpr::ArrayIndex(Box::new(Self::new(
            array, index, item_type,
        ))))
    }

    pub(super) fn resolved_item_type(
        context: &impl ParseContext,
        array: &TypeReference,
    ) -> Option<TypeReference> {
        let element = context.get_array_item_type(array)?;
        // PostgreSQL subscripting does not propagate the anonymous record's
        // typmod/field descriptor. The array definition retains that structure.
        if context.dialect() == crate::ast::Dialect::postgres()
            && element.is_dynamic()
            && element.is_record()
        {
            Some(TypeReference::from_master_type_name("record"))
        } else {
            Some(element)
        }
    }

    pub fn analyze_index<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        arg: SemScalarExpr,
        indices: &crate::syn::AIndices,
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndirectionExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(uidx) = indices.get_uidx().as_inner() else {
            AnalysisError::raise_unexpected_none("aindicies.uidx")?
        };
        let (rexpr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, scalar_context, uidx, tokens)?;
        context = new_context;
        let arg = Self::array_index(arg, rexpr, &mut context)?;
        Ok((arg, context))
    }
}

impl ArrayIndex {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.array.walk(visitor)?;
        self.index.walk(visitor)?;
        Ok(())
    }
}

impl ArrayIndex {
    pub(super) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.walk_assignment_target(visitor)?;
        self.index.walk(visitor)?;
        visitor.subscript(&self.index);
        Ok(())
    }
}

impl ArrayIndex {
    pub(super) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl crate::sem::ParseContext,
    ) -> SemScalarExpr {
        let mut result = parameters.array_access(context, &self.array, &[&self.index], false);
        result.retain_indirection(&IndirectionExpr::index(self));
        result
    }
}
