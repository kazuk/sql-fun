mod array_index;
mod array_slice;
mod member_ref;

use sql_fun_core::IVec;

use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference},
    syn::{ListOpt, Opt, ScanToken},
};

use self::{array_index::ArrayIndex, array_slice::ArraySlice, member_ref::MemberRef};
use super::{AnalyzeScalarExpr, SemScalarExprNode};

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
    fn analyze_indices<TParseContext>(
        context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        arg: SemScalarExpr,
        indices: crate::syn::AIndices,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if indices.get_is_slice() {
            ArraySlice::analyze_slice(context, scalar_context, arg, indices, tokens)
        } else {
            ArrayIndex::analyze_index(context, scalar_context, arg, &indices, tokens)
        }
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

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::AIndirection> for IndirectionExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: crate::syn::AIndirection,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(arg) = syn.get_arg().as_inner() else {
            AnalysisError::raise_unexpected_none("aindirection.arg")?
        };

        let Some(indirection) = syn.get_indirection().as_inner() else {
            AnalysisError::raise_unexpected_none("aindirection.indirection")?
        };
        let (mut arg, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, scalar_context, arg, tokens)?;
        context = new_context;

        for item in indirection {
            if let Some(indices) = item.as_a_indices().as_inner() {
                let (new_arg, new_context) =
                    Self::analyze_indices(context, scalar_context, arg, indices, tokens)?;
                context = new_context;
                arg = SemScalarExpr::Indirection(new_arg);
            } else if let Some(member) = item.as_string().get_sval() {
                arg = SemScalarExpr::Indirection(MemberRef::member_reference(
                    arg,
                    member,
                    &mut context,
                )?);
            } else {
                AnalysisError::raise_unexpected_input(&format!("indirection item: {item:?}"))?;
            }
        }
        Ok((arg, context))
    }
}
