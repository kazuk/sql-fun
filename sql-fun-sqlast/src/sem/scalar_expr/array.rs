use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, ParseContext, SemScalarExpr, TypeReference, analyze_scaler_expr,
        scalar_expr::{AnalyzeScalarExpr, SemScalarExprNode},
    },
    syn::{ListOpt, ScanToken},
};

/// array expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayExpr(Vec<SemScalarExpr>);

impl SemScalarExprNode for ArrayExpr {
    fn get_type(&self) -> Option<crate::sem::TypeReference> {
        let elm_type = self.0.first()?.get_type()?;
        Some(TypeReference::concrete_type_ref(elm_type.full_name(), true))
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::AArrayExpr> for ArrayExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &crate::sem::WithClause,
        from_clause: &crate::sem::FromClause,
        syn: crate::syn::AArrayExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(elements) = syn.get_elements().as_inner() else {
            AnalysisError::raise_unexpected_none("a_array_expr.elements")?
        };

        let mut exprs = Vec::new();
        for element in elements {
            let (sem_elm, new_context) =
                analyze_scaler_expr(context, with_clause, from_clause, element, tokens)?;
            context = new_context;
            exprs.push(sem_elm);
        }

        Ok((SemScalarExpr::Array(Self(exprs)), context))
    }
}
