#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr},
    syn::ScanToken,
};

use ::sql_fun_core::IVec;

impl SemScalarExpr {
    /// analyze [`crate::syn::Node`] as scaler expression
    pub fn analyze_scaler_expr<TParseContext>(
        context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::postgres(&syn, tokens)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::postgres(),
            scalar_context,
        );
        let mut context = context;
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }
}

#[cfg(test)]
mod test_scalar_expr_analyze {
    use crate::sem::ScalarAnalysisContext;
    use crate::{
        sem::{
            ArithExpr,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
        },
        syn::ListOpt,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    #[case("42", Const(Integer(42)))]
    #[case("null", Const(Null))]
    #[case("10 between 1 and 100", ArithExpr::between(
        Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
    ))]
    #[case("10 not between 1 and 100", ArithExpr::not_between(
        Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
    ))]
    fn test_analyze_scalar_expr(
        test_context: TestParseContext,
        #[case] sql_expr: &str,
        #[case] expect_expr: SemScalarExpr,
    ) -> TestResult {
        let syn_builder = SynBuilder::new();
        let node = syn_builder.parse(&format!("select {sql_expr}"));
        let expr = node
            .as_select_stmt()
            .get_target_list()
            .map(|v| v.as_res_target().get_val())
            .unwrap()
            .swap_remove(0);
        let tokens = Vec::default().into();
        let (expr, _result_context) = SemScalarExpr::analyze_scaler_expr(
            test_context,
            &ScalarAnalysisContext::default(),
            expr,
            &tokens,
        )?;
        assert_eq!(expr, expect_expr);

        Ok(())
    }
}

#[cfg(feature = "sqlite")]
impl SemScalarExpr {
    /// analyze #[`crate::syn::sqlite::Parenthesized`]
    pub fn analyze_sqlite_parenthesized<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &crate::syn::sqlite::Parenthesized,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_parenthesized(syn)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprUnary`]
    pub fn analyze_sqlite_unary<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_unary: &crate::syn::sqlite::ExprUnary,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_unary(expr_unary)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprIsNull`]
    pub fn analyze_sqlite_is_null<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_is_null: &crate::syn::sqlite::ExprIsNull,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_is_null(expr_is_null)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprNotNull`]
    pub fn analyze_sqlite_not_null<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_not_null: &crate::syn::sqlite::ExprNotNull,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_not_null(expr_not_null)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBinary`]
    pub fn analyze_sqlite_binary<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_binary: &crate::syn::sqlite::ExprBinary,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_binary(expr_binary)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprCollate`]
    pub fn analyze_sqlite_collate<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_collate: &crate::syn::sqlite::ExprCollate,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_collate(expr_collate)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBetween`]
    pub fn analyze_sqlite_between<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        expr_between: &crate::syn::sqlite::ExprBetween,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite_between(expr_between)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }

    /// analyze sqlite expression
    pub fn analyze_sqlite_scalar_expr<TParseContext>(
        context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &crate::syn::sqlite::Expr,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::sem::unresolved::Expr::sqlite(syn)?;
        let outcome = syntax.resolve(
            &context,
            crate::sem::unresolved::Dialect::sqlite(),
            scalar_context,
        );
        let mut context = context;
        let expression = outcome.report(&mut context)?;
        Ok((expression, context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_sqlite_scalar_expr {
    use crate::sem::ScalarAnalysisContext;
    use crate::{
        sem::{
            ArithExpr,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
        },
        syn::sqlite::{OneSelect, ResultColumn, Stmt},
        test_helpers::{SqliteSynBuilder, TestParseContext, sqlite_syn, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    #[case("42", Const(Integer(42)))]
    #[case("null", Const(Null))]
    #[case("10 between 1 and 100", ArithExpr::between(
        Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
    ))]
    #[case("10 not between 1 and 100", ArithExpr::not_between(
        Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
    ))]
    fn test_analyze_sqlite_scalar_expr(
        test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
        #[case] sql_expr: &str,
        #[case] expect_expr: SemScalarExpr,
    ) -> TestResult {
        let stmt = sqlite_syn.parse_as_stmt(&format!("select {sql_expr}"));
        let Stmt::Select(select) = stmt else {
            panic!("{stmt:?}")
        };
        let OneSelect::Select(select_item) = select.body().select() else {
            panic!("{select:?}")
        };
        let Some(ResultColumn::Expr(result_column_expr)) = select_item.columns().first() else {
            panic!("{select_item:?}")
        };
        let expr = result_column_expr.expr().as_ref();
        let tokens = Vec::default().into();
        let (sem_expr, _new_context) = SemScalarExpr::analyze_sqlite_scalar_expr(
            test_context,
            &ScalarAnalysisContext::default(),
            expr,
            &tokens,
        )?;
        assert_eq!(sem_expr, expect_expr);
        Ok(())
    }
}

/// analyze [`crate::syn::Node`] as scaler expression
#[deprecated = "use SemScalarExpr::analyze_scaler_expr"]
pub fn analyze_scaler_expr<TParseContext>(
    context: TParseContext,
    scalar_context: &ScalarAnalysisContext<'_>,
    syn: crate::syn::Node,
    tokens: &IVec<ScanToken>,
) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    SemScalarExpr::analyze_scaler_expr(context, scalar_context, syn, tokens)
}
