mod array_index;
mod array_slice;
mod array_subscripts;
mod member_ref;

pub use self::array_subscripts::{ArraySubscript, ArraySubscripts};

use self::{array_index::ArrayIndex, array_slice::ArraySlice, member_ref::MemberRef};

use crate::{
    context::ParseContext,
    sem::{AnalysisError, SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode},
};

/// Indirect expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndirectionExpr {
    /// Array element index access expression
    ArrayIndex(Box<ArrayIndex>),
    /// Array range slice
    ArraySlice(Box<ArraySlice>),
    /// One consecutive PostgreSQL group of multiple array subscripts.
    ArraySubscripts(Box<ArraySubscripts>),
    /// composite type member
    MemberRef(Box<MemberRef>),
}

impl IndirectionExpr {
    pub(super) fn subscripts(value: &ArraySubscripts) -> Self {
        Self::ArraySubscripts(Box::new(value.clone()))
    }

    /// Consecutive subscripts, when this operation contains multiple dimensions.
    pub fn as_array_subscripts(&self) -> Option<&ArraySubscripts> {
        match self {
            Self::ArraySubscripts(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn resolved_subscripts(
        context: &mut impl ParseContext,
        base: &SemScalarExpr,
        subscripts: &[ArraySubscript],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let group = ArraySubscripts::resolve(context, base, subscripts)?;
        Ok(SemScalarExpr::indirection(&Self::subscripts(&group)))
    }
    pub(super) fn index(value: &ArrayIndex) -> Self {
        Self::ArrayIndex(Box::new(value.clone()))
    }
    pub(super) fn slice(value: &ArraySlice) -> Self {
        Self::ArraySlice(Box::new(value.clone()))
    }
    pub(crate) fn set_result_type(&mut self, result: &Option<TypeReference>) {
        match self {
            Self::ArrayIndex(value) => value.set_item_type(result),
            Self::ArraySlice(_) => {}
            Self::ArraySubscripts(value) => value.set_result_type(result),
            Self::MemberRef(value) => value.set_member_type(result),
        }
    }
    pub(crate) fn resolved_index<TContext: ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        index: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArrayIndex::index_resolved(
            base, index, context,
        )?))
    }

    pub(crate) fn resolved_slice<TContext: ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArraySlice::slice_resolved(
            base, lower, upper, context,
        )?))
    }

    pub(crate) fn resolved_member<TContext: ParseContext>(
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
            Self::ArraySubscripts(value) => value.result_type().cloned(),
            Self::MemberRef(mem_ref) => mem_ref.member_type().clone(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self {
            // A non-null container can hold NULL or have no element at the index.
            Self::ArrayIndex(_) => Some(false),
            Self::ArraySlice(slice) => slice.array_is_not_null(),
            Self::ArraySubscripts(value) => {
                if value.is_slice() {
                    value.array().is_not_null()
                } else {
                    Some(false)
                }
            }
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
            Self::ArraySubscripts(value) => value.walk(visitor)?,
            Self::MemberRef(v) => v.walk(visitor)?,
        }
        Ok(())
    }
}

impl IndirectionExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl crate::sem::ParseContext,
    ) -> Option<SemScalarExpr> {
        match self {
            Self::ArrayIndex(value) => Some(value.collect_constraints(parameters, context)),
            Self::ArraySlice(value) => Some(value.collect_constraints(parameters, context)),
            Self::ArraySubscripts(value) => Some(value.collect_constraints(parameters, context)),
            Self::MemberRef(value) => value.collect_constraints(parameters),
        }
    }
}

impl IndirectionExpr {
    pub(crate) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::ArrayIndex(target) => target.walk_assignment_target(visitor),
            Self::ArraySlice(target) => target.walk_assignment_target(visitor),
            Self::ArraySubscripts(value) => value.walk_assignment_target(visitor),
            Self::MemberRef(target) => target.walk_assignment_target(visitor),
        }
    }
}
