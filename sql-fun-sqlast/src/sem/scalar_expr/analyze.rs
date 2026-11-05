#[cfg(feature = "sqlite")]
use crate::{
    SqliteToken,
    syn::sqlite::{
        Expr as SqliteExpr, ExprBetween, ExprBinary, ExprCollate, ExprIsNull, ExprNotNull,
        ExprUnary, Parenthesized,
    },
};

use crate::{
    ast::{Expr, TryFromWithTokens},
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr},
    syn::{Node, ScanToken},
};

use ::sql_fun_core::IVec;

impl SemScalarExpr {
    /// analyze [`crate::syn::Node`] as scaler expression
    pub fn analyze_scaler_expr<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from_with_tokens(&syn, tokens)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }
}

#[cfg(feature = "sqlite")]
impl SemScalarExpr {
    /// analyze #[`crate::syn::sqlite::Parenthesized`]
    pub fn analyze_sqlite_parenthesized<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &Parenthesized,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(syn)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprUnary`]
    pub fn analyze_sqlite_unary<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_unary: &ExprUnary,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_unary)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprIsNull`]
    pub fn analyze_sqlite_is_null<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_is_null: &ExprIsNull,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_is_null)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprNotNull`]
    pub fn analyze_sqlite_not_null<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_not_null: &ExprNotNull,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_not_null)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBinary`]
    pub fn analyze_sqlite_binary<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_binary: &ExprBinary,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_binary)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprCollate`]
    pub fn analyze_sqlite_collate<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_collate: &ExprCollate,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_collate)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBetween`]
    pub fn analyze_sqlite_between<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_between: &ExprBetween,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(expr_between)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }

    /// analyze sqlite expression
    pub fn analyze_sqlite_scalar_expr<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &SqliteExpr,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(syn)?;
        let outcome = Self::from_ast(&syntax, &mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }
}
