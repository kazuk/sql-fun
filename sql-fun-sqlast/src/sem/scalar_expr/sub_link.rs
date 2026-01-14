use sql_fun_core::IVec;

use crate::{
    AstAndContextPair,
    sem::{
        AnalysisError, FromClause, ParseContext, SelectStatement, SemScalarExpr, TypeReference,
        WithClause, analyze_select,
    },
    syn::{Opt, ScanToken},
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

/// Sub-query expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubLinkExpr {
    subselect: Box<SelectStatement>,
    type_ref: TypeReference,
}

impl SemScalarExprNode for SubLinkExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.type_ref.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::SubLink> for SubLinkExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        _with_clause: &WithClause,
        _from_clause: &FromClause,
        syn: crate::syn::SubLink,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(subselect) = syn.get_subselect().as_select_stmt().as_inner() else {
            AnalysisError::raise_unexpected_none("sublink.subselect")?
        };
        let AstAndContextPair(select_stmt, new_context) =
            analyze_select(context, subselect, tokens)?;
        context = new_context;
        let Some(select_stmt) = select_stmt.into_select_statemtnt() else {
            AnalysisError::raise_unexpected_none("select_statemnt")?
        };

        let type_ref = select_stmt.returning_composite_type()?;

        Ok((
            SemScalarExpr::SubLink(Self {
                subselect: Box::new(select_stmt),
                type_ref,
            }),
            context,
        ))
    }
}
