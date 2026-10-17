mod array_index;
mod array_slice;
mod member_ref;

use self::{array_index::ArrayIndex, array_slice::ArraySlice, member_ref::MemberRef};

use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{AnalysisError, SemScalarExpr, TypeReference};

/// Indirect expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndirectionExpr {
    /// Array element index access expression
    ArrayIndex(Box<ArrayIndex>),
    /// Array range slice
    ArraySlice(Box<ArraySlice>),
    /// composite type member
    MemberRef(Box<MemberRef>),
}

impl IndirectionExpr {
    pub(crate) fn resolved_index<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        index: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArrayIndex::index_resolved(
            base, index, context,
        )?))
    }

    pub(crate) fn resolved_slice<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArraySlice::slice_resolved(
            base, lower, upper, context,
        )?))
    }

    pub(crate) fn resolved_member<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        name: &str,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&MemberRef::member_resolved(
            base,
            name.to_owned(),
            context,
        )?))
    }
}

impl SemScalarExprNode for IndirectionExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            Self::ArrayIndex(ind) => ind.item_type().clone(),
            Self::ArraySlice(slice) => slice.array_type().clone(),
            Self::MemberRef(mem_ref) => mem_ref.member_type().clone(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self {
            Self::ArrayIndex(ind) => ind.array().is_not_null(),
            Self::ArraySlice(slice) => slice.array_is_not_null(),
            Self::MemberRef(mem_ref) => mem_ref.base_is_not_null(),
        }
    }
}

impl IndirectionExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::ArrayIndex(v) => v.walk(visitor)?,
            Self::ArraySlice(v) => v.walk(visitor)?,
            Self::MemberRef(v) => v.walk(visitor)?,
        }
        Ok(())
    }
}

impl IndirectionExpr {
    fn requires_type_inference(&self) -> bool {
        match self {
            Self::ArrayIndex(value) => value.requires_type_inference(),
            Self::ArraySlice(value) => value.requires_type_inference(),
            Self::MemberRef(value) => value.requires_type_inference(),
        }
    }
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) {
        if self.requires_type_inference() {
            parameters.constrain_indirection(self);
        }
    }
}
