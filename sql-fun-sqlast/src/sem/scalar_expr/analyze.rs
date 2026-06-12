use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::{
    SqliteToken,
    syn::sqlite::{Operator, UnaryOperator},
};
use crate::{
    sem::{
        AnalysisError, ArithExpr, ArrayExpr, BooleanExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, FuncCallExpr, IndirectionExpr, MinMaxExpr, NamedArgExpr, NullTestExpr,
        ParamRef, ParseContext, ScalarAnalysisContext, ScalarConstExpr, SemScalarExpr,
        SqlValueExpr, SubLinkExpr, TypeCastExpr,
        scalar_expr::{AnalyzeScalarExpr, ListExpr},
    },
    syn::{Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use super::boolean::{BooleanExprBody, NullTestType};

#[cfg(feature = "sqlite")]
fn normalize_sqlite_string_literal(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        value[1..value.len() - 1].replace("''", "'")
    } else {
        String::from(value)
    }
}

#[cfg(feature = "sqlite")]
fn analyze_sqlite_literal(literal: &crate::syn::sqlite::Literal) -> SemScalarExpr {
    use crate::syn::sqlite::Literal;

    match literal {
        Literal::Numeric(value) => {
            if let Ok(integer) = value.value_text().parse::<i32>() {
                SemScalarExpr::Const(ScalarConstExpr::Integer(integer))
            } else {
                SemScalarExpr::Const(ScalarConstExpr::Float(String::from(value.value_text())))
            }
        }
        Literal::String(value) => SemScalarExpr::Const(ScalarConstExpr::String(
            normalize_sqlite_string_literal(value.value()),
        )),
        Literal::Null(_) => ScalarConstExpr::null(),
        Literal::True(value) | Literal::False(value) => {
            SemScalarExpr::Const(ScalarConstExpr::Bool(value.value()))
        }
        Literal::CurrentDate(_) => SemScalarExpr::SqlValue(SqlValueExpr::CurrentDate),
        Literal::CurrentTime(_) => SemScalarExpr::SqlValue(SqlValueExpr::CurrentTime(None)),
        Literal::CurrentTimestamp(_) => {
            SemScalarExpr::SqlValue(SqlValueExpr::CurrentTimestamp(None))
        }
        Literal::Blob(_) => todo!(),
        Literal::Keyword(_) => todo!(),
    }
}

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
        if syn.is_a_const() {
            if let Some(aconst) = syn.as_a_const().as_inner() {
                ScalarConstExpr::analyze_scalar_expr(context, scalar_context, aconst, tokens)
            } else {
                Ok((ScalarConstExpr::null(), context))
            }
        } else if let Some(col_ref) = syn.as_column_ref().as_inner() {
            ColumnReferenceExpr::analyze_scalar_expr(context, scalar_context, col_ref, tokens)
        } else if let Some(fun_call) = syn.as_func_call().as_inner() {
            FuncCallExpr::analyze_scalar_expr(context, scalar_context, fun_call, tokens)
        } else if let Some(a_expr) = syn.as_a_expr().as_inner() {
            ArithExpr::analyze_scalar_expr(context, scalar_context, a_expr, tokens)
        } else if let Some(cast) = syn.as_type_cast().as_inner() {
            TypeCastExpr::analyze_scalar_expr(context, scalar_context, cast, tokens)
        } else if let Some(case) = syn.as_case_expr().as_inner() {
            CaseExpr::analyze_scalar_expr(context, scalar_context, case, tokens)
        } else if let Some(bexpr) = syn.as_bool_expr().as_inner() {
            BooleanExpr::analyze_scalar_expr(context, scalar_context, bexpr, tokens)
        } else if let Some(nt) = syn.as_null_test().as_inner() {
            NullTestExpr::analyze_scalar_expr(context, scalar_context, nt, tokens)
        } else if let Some(sl) = syn.as_sub_link().as_inner() {
            SubLinkExpr::analyze_scalar_expr(context, scalar_context, sl, tokens)
        } else if let Some(ce) = syn.as_coalesce_expr().as_inner() {
            CoalesceExpr::analyze_scalar_expr(context, scalar_context, ce, tokens)
        } else if let Some(mme) = syn.as_min_max_expr().as_inner() {
            MinMaxExpr::analyze_scalar_expr(context, scalar_context, mme, tokens)
        } else if let Some(ce) = syn.as_collate_expr().as_inner() {
            CollateExpr::analyze_scalar_expr(context, scalar_context, ce, tokens)
        } else if let Some(svf) = syn.as_sqlvalue_function().as_inner() {
            SqlValueExpr::analyze_scalar_expr(context, scalar_context, svf, tokens)
        } else if let Some(pr) = syn.as_param_ref().as_inner() {
            ParamRef::analyze_scalar_expr(context, scalar_context, pr, tokens)
        } else if let Some(nae) = syn.as_named_arg_expr().as_inner() {
            NamedArgExpr::analyze_scalar_expr(context, scalar_context, nae, tokens)
        } else if let Some(ind) = syn.as_a_indirection().as_inner() {
            IndirectionExpr::analyze_scalar_expr(context, scalar_context, ind, tokens)
        } else if let Some(arr) = syn.as_a_array_expr().as_inner() {
            ArrayExpr::analyze_scalar_expr(context, scalar_context, arr, tokens)
        } else if let Some(list) = syn.as_list().as_inner() {
            ListExpr::analyze_scalar_expr(context, scalar_context, list, tokens)
        } else {
            AnalysisError::raise_unexpected_input(&format!(
                "analyze_scaler_expr unexpected node :{syn:?} "
            ))
        }
    }
}

#[cfg(test)]
mod test_scalar_expr_analyze {
    use testresult::TestResult;

    use crate::{
        sem::{
            ArithExpr, FromClause,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
            WithClause,
        },
        syn::ListOpt,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

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
            &WithClause::default(),
            &FromClause::default(),
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
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: &crate::syn::sqlite::Parenthesized,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let expr = if let [expr] = syn.expr() {
            let (expr, new_context) =
                Self::analyze_sqlite_scalar_expr(context, with_clause, from_clause, expr, tokens)?;
            context = new_context;
            expr
        } else {
            todo!()
        };
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprUnary`]
    pub fn analyze_sqlite_unary<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_unary: &crate::syn::sqlite::ExprUnary,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_unary.expr(),
            tokens,
        )?;
        context = new_context;

        let expr = match expr_unary.unary_operator() {
            UnaryOperator::Positive => expr,
            UnaryOperator::Negative => match expr {
                Self::Const(ScalarConstExpr::Integer(value)) => {
                    Self::Const(ScalarConstExpr::Integer(-value))
                }
                Self::Const(ScalarConstExpr::Float(value)) => {
                    Self::Const(ScalarConstExpr::Float(format!("-{value}")))
                }
                _ => todo!(),
            },
            UnaryOperator::Not => Self::Boolean(BooleanExpr::new(BooleanExprBody::not(&expr))),
            UnaryOperator::BitwiseNot => todo!(),
        };
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprIsNull`]
    pub fn analyze_sqlite_is_null<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_is_null: &crate::syn::sqlite::ExprIsNull,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_is_null.expr(),
            tokens,
        )?;
        context = new_context;
        let expr = Self::NullTest(NullTestExpr::new(NullTestType::IsNull, expr));
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprNotNull`]
    pub fn analyze_sqlite_not_null<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_not_null: &crate::syn::sqlite::ExprNotNull,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_not_null.expr(),
            tokens,
        )?;
        context = new_context;
        let expr = Self::NullTest(NullTestExpr::new(NullTestType::IsNotNull, expr));
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBinary`]
    pub fn analyze_sqlite_binary<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_binary: &crate::syn::sqlite::ExprBinary,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (left, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_binary.left(),
            tokens,
        )?;
        context = new_context;
        let (right, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_binary.right(),
            tokens,
        )?;
        context = new_context;

        let expr = match expr_binary.operator() {
            Operator::And => Self::Boolean(BooleanExpr::new(BooleanExprBody::and(&[left, right]))),
            Operator::Or => Self::Boolean(BooleanExpr::new(BooleanExprBody::or(&[left, right]))),
            _ => todo!(),
        };
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprCollate`]
    pub fn analyze_sqlite_collate<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_collate: &crate::syn::sqlite::ExprCollate,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_collate.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((expr, context))
    }

    /// analyze #[`crate::syn::sqlite::ExprBetween`]
    pub fn analyze_sqlite_between<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        expr_between: &crate::syn::sqlite::ExprBetween,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (lhs, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_between.lhs(),
            tokens,
        )?;
        context = new_context;
        let (start, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_between.start(),
            tokens,
        )?;
        context = new_context;
        let (end, new_context) = Self::analyze_sqlite_scalar_expr(
            context,
            with_clause,
            from_clause,
            expr_between.end(),
            tokens,
        )?;
        context = new_context;
        if *expr_between.not() {
            Ok((ArithExpr::not_between(lhs, (start, end)), context))
        } else {
            Ok((ArithExpr::between(lhs, (start, end)), context))
        }
    }

    /// analyze sqlite expression
    pub fn analyze_sqlite_scalar_expr<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: &crate::syn::sqlite::Expr,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::syn::sqlite::Expr;

        match syn {
            Expr::Literal(literal) => Ok((analyze_sqlite_literal(literal), context)),
            Expr::Parenthesized(parenthesized) => Self::analyze_sqlite_parenthesized(
                context,
                with_clause,
                from_clause,
                parenthesized,
                tokens,
            ),
            Expr::Unary(expr_unary) => {
                Self::analyze_sqlite_unary(context, with_clause, from_clause, expr_unary, tokens)
            }
            Expr::IsNull(expr_is_null) => Self::analyze_sqlite_is_null(
                context,
                with_clause,
                from_clause,
                expr_is_null,
                tokens,
            ),
            Expr::NotNull(expr_not_null) => Self::analyze_sqlite_not_null(
                context,
                with_clause,
                from_clause,
                expr_not_null,
                tokens,
            ),
            Expr::Binary(expr_binary) => {
                Self::analyze_sqlite_binary(context, with_clause, from_clause, expr_binary, tokens)
            }
            Expr::Collate(expr_collate) => Self::analyze_sqlite_collate(
                context,
                with_clause,
                from_clause,
                expr_collate,
                tokens,
            ),
            Expr::Between(expr_between) => Self::analyze_sqlite_between(
                context,
                with_clause,
                from_clause,
                expr_between,
                tokens,
            ),
            Expr::Register(_) => todo!(),
            Expr::Case(_) => todo!(),
            Expr::Cast(expr_cast) => TypeCastExpr::analyze_sqlite_cast(
                context,
                with_clause,
                from_clause,
                expr_cast,
                tokens,
            ),
            Expr::DoublyQualified(_) => todo!(),
            Expr::Exists(_) => todo!(),
            Expr::FunctionCall(_) => todo!(),
            Expr::FunctionCallStar(_) => todo!(),
            Expr::Id(_) => todo!(),
            Expr::Column(_) => todo!(),
            Expr::RowId(_) => todo!(),
            Expr::InList(_) => todo!(),
            Expr::InSelect(_) => todo!(),
            Expr::InTable(_) => todo!(),
            Expr::Like(_) => todo!(),
            Expr::Name(_) => todo!(),
            Expr::Qualified(_) => todo!(),
            Expr::Raise(_) => todo!(),
            Expr::Subquery(_) => todo!(),
            Expr::Variable(_) => todo!(),
            Expr::SubqueryResult(_) => todo!(),
            Expr::FieldAccess(_) => todo!(),
            Expr::Default => todo!(),
            Expr::Array(_) => todo!(),
            Expr::Subscript(_) => todo!(),
        }
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_sqlite_scalar_expr {
    use testresult::TestResult;

    use crate::{
        sem::{
            ArithExpr, FromClause,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
            WithClause,
        },
        syn::sqlite::{OneSelect, ResultColumn, Stmt},
        test_helpers::{SqliteSynBuilder, TestParseContext, sqlite_syn, test_context},
    };

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
        let Some(ResultColumn::Expr(result_column_expr)) = select_item.columns().get(0) else {
            panic!("{select_item:?}")
        };
        let expr = result_column_expr.expr().as_ref();
        let tokens = Vec::default().into();
        let (sem_expr, _new_context) = SemScalarExpr::analyze_sqlite_scalar_expr(
            test_context,
            &WithClause::default(),
            &FromClause::default(),
            expr,
            &tokens,
        )?;
        assert_eq!(sem_expr, expect_expr);
        Ok(())
    }
}

#[cfg(test)]
mod test_scalar_expr_analyze {
    use crate::sem::ScalarAnalysisContext;
    use testresult::TestResult;

    use crate::{
        sem::{
            ArithExpr,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
        },
        syn::ListOpt,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

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
