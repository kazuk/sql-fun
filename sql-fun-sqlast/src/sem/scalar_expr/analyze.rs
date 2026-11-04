#[cfg(feature = "sqlite")]
use crate::SqliteToken;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Expr as SqliteExpr;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprBetween;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprBinary;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprCollate;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprIsNull;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprNotNull;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprUnary;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Parenthesized;
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
        let outcome = syntax.resolve(&mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }
}

#[cfg(test)]
mod test_func_call_expr_analyze_scalar_expr {
    use crate::test_helpers::function_syntax::func_name_tokens;

    use crate::{
        names::SchemaName,
        sem::{
            FromClause, FunctionParam, OverloadVariant, PgBuiltInType, ScalarAnalysisContext,
            SemScalarExpr, WithClause,
        },
        syn::{Node, NodeInner, NodeList},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_scalar_expr_reports_missing_function(
        mut test_context: TestParseContext,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);

        let builder = SynBuilder::new();
        let arg_expr = Node::from(NodeInner::AConst(builder.const_int4(1)));
        let args = NodeList::from(vec![arg_expr]);
        let func_name_node = builder.as_string_node(builder.string("missing_func"));
        let funcname = NodeList::from(vec![func_name_node]);
        let func_call = builder.func_call(funcname, args);

        let tokens = func_name_tokens();
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let (expr, context) = SemScalarExpr::analyze_scaler_expr(
            test_context,
            &ScalarAnalysisContext::new(&with_clause, &from_clause),
            Node::from(NodeInner::from(func_call)),
            &tokens,
        )?;

        assert_eq!(1, context.reported_problem_count());
        let SemScalarExpr::FuncCall(func_call_expr) = expr else {
            panic!("expected func call expression");
        };
        assert!(func_call_expr.overload().is_none());
        assert_eq!(1, func_call_expr.args().len());
        assert!(matches!(
            func_call_expr.args()[0].arg_expr(),
            SemScalarExpr::Const(_)
        ));
        assert!(func_call_expr.args()[0].arg_definition().is_none());
        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_scalar_expr_resolves_overload(
        mut test_context: TestParseContext,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let int4 = PgBuiltInType::int4();
        test_context.setup_type(int4.clone());

        let params = vec![FunctionParam::new_input_param(
            &Some("arg".to_string()),
            &Some(int4.clone()),
            &None,
        )];
        let overload = OverloadVariant::new(&Some(int4.clone()), &params, false, false);
        test_context.setup_function("test_func", std::slice::from_ref(&overload));

        let builder = SynBuilder::new();
        let arg_expr = Node::from(NodeInner::AConst(builder.const_int4(1)));
        let args = NodeList::from(vec![arg_expr]);
        let func_name_node = builder.as_string_node(builder.string("test_func"));
        let funcname = NodeList::from(vec![func_name_node]);
        let func_call = builder.func_call(funcname, args);

        let tokens = func_name_tokens();
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let (expr, context) = SemScalarExpr::analyze_scaler_expr(
            test_context,
            &ScalarAnalysisContext::new(&with_clause, &from_clause),
            Node::from(NodeInner::from(func_call)),
            &tokens,
        )?;

        assert_eq!(0, context.reported_problem_count());
        let SemScalarExpr::FuncCall(func_call_expr) = expr else {
            panic!("expected func call expression");
        };
        assert_eq!(Some(&overload), func_call_expr.overload().as_ref());
        assert!(func_call_expr.args()[0].arg_definition().is_some());
        assert!(matches!(
            func_call_expr.args()[0].arg_expr(),
            SemScalarExpr::Const(_)
        ));
        Ok(())
    }
}

#[cfg(test)]
mod test_scalar_expr_analyze {
    use crate::{
        sem::{
            ScalarAnalysisContext,
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
        syn: &Parenthesized,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from(syn)?;
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
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
        let outcome = syntax.resolve(&mut context, scalar_context);
        let expression = outcome?;
        Ok((expression, context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_sqlite_scalar_expr {
    use crate::ast::Dialect;

    use crate::{
        sem::{
            ArithExpr, ScalarAnalysisContext,
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
            test_context.with_dialect(Dialect::sqlite()),
            &ScalarAnalysisContext::default(),
            expr,
            &tokens,
        )?;
        assert_eq!(sem_expr, expect_expr);
        Ok(())
    }
}
