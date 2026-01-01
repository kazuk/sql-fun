mod array_index;
mod array_slice;
mod member_ref;

use crate::{
    sem::{
        AnalysisError, FromClause, ParseContext, SemScalarExpr, TypeReference, WithClause,
        analyze_scaler_expr,
    },
    syn::{ListOpt, Opt},
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
    fn analyze_indicies<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        arg: SemScalarExpr,
        indicies: crate::syn::AIndices,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if indicies.get_is_slice() {
            ArraySlice::analyze_slice(context, with_clause, from_clause, arg, indicies)
        } else {
            ArrayIndex::analyze_index(context, with_clause, from_clause, arg, &indicies)
        }
    }
}

impl SemScalarExprNode for IndirectionExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            IndirectionExpr::ArrayIndex(ind) => ind.item_type().clone(),
            IndirectionExpr::ArraySlice(slice) => slice.array_type().clone(),
            IndirectionExpr::MemberRef(mem_ref) => mem_ref.member_type().clone(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self {
            IndirectionExpr::ArrayIndex(ind) => ind.array().is_not_null(),
            IndirectionExpr::ArraySlice(slice) => slice.array_is_not_null(),
            IndirectionExpr::MemberRef(mem_ref) => mem_ref.base_is_not_null(),
        }
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::AIndirection> for IndirectionExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::AIndirection,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(arg) = syn.get_arg().as_inner() else {
            AnalysisError::raise_unexpected_none("aindirection.arg")?
        };

        let Some(indirection) = syn.get_indirection().as_inner() else {
            AnalysisError::raise_unexpected_none("aindirection.indirection")?
        };
        let (mut arg, new_context) = analyze_scaler_expr(context, with_clause, from_clause, arg)?;
        context = new_context;

        for item in indirection {
            if let Some(indicies) = item.as_a_indices().as_inner() {
                let (new_arg, new_context) =
                    Self::analyze_indicies(context, with_clause, from_clause, arg, indicies)?;
                context = new_context;
                arg = SemScalarExpr::Indirection(new_arg);
            } else if let Some(member) = item.as_string().get_sval() {
                arg = SemScalarExpr::Indirection(MemberRef::member_reference(
                    arg,
                    member,
                    &mut context,
                ));
            } else {
                AnalysisError::raise_unexpected_input(&format!("indirection item: {item:?}"))?;
            }
        }
        Ok((arg, context))
    }
}
