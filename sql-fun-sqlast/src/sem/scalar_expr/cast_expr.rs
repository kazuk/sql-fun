#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::sem::scalar_expr::SemScalarExprNode;
#[cfg(feature = "sqlite")]
use crate::sem::{AnalysisError, ParseContext, ScalarAnalysisContext};
use crate::sem::{SemScalarExpr, TypeReference};

#[cfg(feature = "sqlite")]
use ::sql_fun_core::IVec;

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
    pub(super) fn from_resolved(target_type: &TypeReference, arg: &SemScalarExpr) -> Self {
        Self {
            target_type: target_type.clone(),
            arg: Box::new(arg.clone()),
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

#[cfg(feature = "sqlite")]
impl TypeCastExpr {
    /// analyze [`crate::syn::sqlite::ExprCast`]
    pub fn analyze_sqlite_cast<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &crate::syn::sqlite::ExprCast,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Expr::try_from(syn)?;
        let result = syntax.resolve(&context, crate::ast::Dialect::sqlite(), scalar_context);
        Ok((result.report(&mut context)?, context))
    }
}
