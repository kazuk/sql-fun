use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, FullName, ParseContext, SemScalarExpr,
        TypeReference, WithClause, analyze_scaler_expr,
    },
    syn::Opt,
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

/// implicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImplicitCastExpr {
    value: Box<SemScalarExpr>,
    cast_target: TypeReference,
}

impl ImplicitCastExpr {
    /// create a implicit cast expression
    pub fn new(value: &SemScalarExpr, cast_target: &TypeReference) -> Self {
        Self {
            value: Box::new(value.clone()),
            cast_target: cast_target.clone(),
        }
    }
}

impl SemScalarExprNode for ImplicitCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.cast_target.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.value.is_not_null()
    }
}

/// explicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeCastExpr {
    target_type: TypeReference,
    arg: Box<SemScalarExpr>,
}

impl TypeCastExpr {
    fn new(target_type: TypeReference, arg: SemScalarExpr) -> TypeCastExpr {
        Self {
            target_type,
            arg: Box::new(arg),
        }
    }
}

impl SemScalarExprNode for TypeCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.target_type.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::TypeCast> for TypeCastExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::TypeCast,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(arg) = syn.get_arg().as_inner() else {
            AnalysisError::raise_unexpected_none("typecast.arg")?
        };
        let Some(target_type) = syn.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("typecast.type_name")?
        };
        let target_type = TypeReference::from_full_name(FullName::try_from(target_type)?);

        let (arg, new_context) = analyze_scaler_expr(context, with_clause, from_clause, arg)?;
        context = new_context;

        if let Some(from_type) = arg.get_type() {
            let cast = context.get_explicit_cast(&from_type, &target_type);
            if cast.is_none() {
                context.report_problem(AnalysisProblem::explicit_cast_not_defined(
                    &from_type,
                    &target_type,
                ));
            }
        }

        Ok((
            SemScalarExpr::TypeCast(Self::new(target_type, arg)),
            context,
        ))
    }
}
