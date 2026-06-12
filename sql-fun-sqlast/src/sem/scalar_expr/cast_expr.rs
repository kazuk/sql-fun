use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FullName, ParseContext, ScalarAnalysisContext,
        SemScalarExpr, TypeReference,
    },
    syn::{Opt, ScanToken},
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
    #[must_use]
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
    fn new(target_type: TypeReference, arg: SemScalarExpr) -> Self {
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
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: crate::syn::TypeCast,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(arg) = syn.get_arg().as_inner() else {
            AnalysisError::raise_unexpected_none("typecast.arg")?
        };
        let Some(target_type) = syn.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("typecast.type_name")?
        };
        let target_type = TypeReference::from_full_name(FullName::try_from(target_type)?);

        let (arg, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, scalar_context, arg, tokens)?;
        context = new_context;

        if let Some(from_type) = arg.get_type() {
            let cast = context.get_explicit_cast(&from_type, &target_type);
            if cast.is_none() {
                context.report_problem(AnalysisProblem::explicit_cast_not_defined(
                    &from_type,
                    &target_type,
                ))?;
            }
        }

        Ok((
            SemScalarExpr::TypeCast(Self::new(target_type, arg)),
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl TypeCastExpr {
    /// analyze [`crate::syn::sqlite::ExprCast`]
    pub fn analyze_sqlite_cast<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: &crate::syn::sqlite::ExprCast,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(ty) = syn.type_name() else {
            AnalysisError::raise_unexpected_none("ExprCast.type_name")?;
            AnalysisError::never();
        };
        let target_type_name = FullName::from_local_name(ty.name());
        if context.get_type(&target_type_name).is_none() {
            context.report_problem(AnalysisProblem::type_not_found(&target_type_name))?;
        };

        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            syn.expr(),
            tokens,
        )?;
        context = new_context;

        let target_type = TypeReference::from_full_name(target_type_name);
        Ok((
            SemScalarExpr::TypeCast(Self::new(target_type, expr)),
            context,
        ))
    }
}
