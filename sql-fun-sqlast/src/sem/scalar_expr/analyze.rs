use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::{
    sem::{
        AnalysisError, ArithExpr, ArrayExpr, BooleanExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, FromClause, FuncCallExpr, IndirectionExpr, MinMaxExpr, NamedArgExpr,
        NullTestExpr, ParamRef, ParseContext, ScalarConstExpr, SemScalarExpr, SqlValueExpr,
        SubLinkExpr, TypeCastExpr, WithClause, scalar_expr::AnalyzeScalarExpr,
    },
    syn::{Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use super::boolean::{BooleanExprBody, NullTestType};

#[cfg(feature = "sqlite")]
fn unsupported_sqlite_expr(kind: &str) -> SemScalarExpr {
    SemScalarExpr::Unexpected(format!("sqlite expr not yet supported: {kind}"))
}

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
        Literal::Blob(_) => unsupported_sqlite_expr("blob literal"),
        Literal::Keyword(_) => unsupported_sqlite_expr("keyword literal"),
    }
}

impl SemScalarExpr {
    /// analyze [`crate::syn::Node`] as scaler expression
    pub fn analyze_scaler_expr<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if syn.is_a_const() {
            if let Some(aconst) = syn.as_a_const().as_inner() {
                ScalarConstExpr::analyze_scalar_expr(
                    context,
                    with_clause,
                    from_clause,
                    aconst,
                    tokens,
                )
            } else {
                Ok((ScalarConstExpr::null(), context))
            }
        } else if let Some(col_ref) = syn.as_column_ref().as_inner() {
            ColumnReferenceExpr::analyze_scalar_expr(
                context,
                with_clause,
                from_clause,
                col_ref,
                tokens,
            )
        } else if let Some(fun_call) = syn.as_func_call().as_inner() {
            FuncCallExpr::analyze_scalar_expr(context, with_clause, from_clause, fun_call, tokens)
        } else if let Some(a_expr) = syn.as_a_expr().as_inner() {
            ArithExpr::analyze_scalar_expr(context, with_clause, from_clause, a_expr, tokens)
        } else if let Some(cast) = syn.as_type_cast().as_inner() {
            TypeCastExpr::analyze_scalar_expr(context, with_clause, from_clause, cast, tokens)
        } else if let Some(case) = syn.as_case_expr().as_inner() {
            CaseExpr::analyze_scalar_expr(context, with_clause, from_clause, case, tokens)
        } else if let Some(bexpr) = syn.as_bool_expr().as_inner() {
            BooleanExpr::analyze_scalar_expr(context, with_clause, from_clause, bexpr, tokens)
        } else if let Some(nt) = syn.as_null_test().as_inner() {
            NullTestExpr::analyze_scalar_expr(context, with_clause, from_clause, nt, tokens)
        } else if let Some(sl) = syn.as_sub_link().as_inner() {
            SubLinkExpr::analyze_scalar_expr(context, with_clause, from_clause, sl, tokens)
        } else if let Some(ce) = syn.as_coalesce_expr().as_inner() {
            CoalesceExpr::analyze_scalar_expr(context, with_clause, from_clause, ce, tokens)
        } else if let Some(mme) = syn.as_min_max_expr().as_inner() {
            MinMaxExpr::analyze_scalar_expr(context, with_clause, from_clause, mme, tokens)
        } else if let Some(ce) = syn.as_collate_expr().as_inner() {
            CollateExpr::analyze_scalar_expr(context, with_clause, from_clause, ce, tokens)
        } else if let Some(svf) = syn.as_sqlvalue_function().as_inner() {
            SqlValueExpr::analyze_scalar_expr(context, with_clause, from_clause, svf, tokens)
        } else if let Some(pr) = syn.as_param_ref().as_inner() {
            ParamRef::analyze_scalar_expr(context, with_clause, from_clause, pr, tokens)
        } else if let Some(nae) = syn.as_named_arg_expr().as_inner() {
            NamedArgExpr::analyze_scalar_expr(context, with_clause, from_clause, nae, tokens)
        } else if let Some(ind) = syn.as_a_indirection().as_inner() {
            IndirectionExpr::analyze_scalar_expr(context, with_clause, from_clause, ind, tokens)
        } else if let Some(arr) = syn.as_a_array_expr().as_inner() {
            ArrayExpr::analyze_scalar_expr(context, with_clause, from_clause, arr, tokens)
        } else {
            AnalysisError::raise_unexpected_input(&format!(
                "analyze_scaler_expr unexpected node :{syn:?} "
            ))
        }
    }

    /// analyze sqlite expression
    #[cfg(feature = "sqlite")]
    #[expect(clippy::only_used_in_recursion)]
    pub fn analyze_sqlite_scaler_expr<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: &crate::syn::sqlite::Expr,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::syn::sqlite::{Expr, Operator, UnaryOperator};

        let expr = match syn {
            Expr::Literal(literal) => analyze_sqlite_literal(literal),
            Expr::Parenthesized(parenthesized) => {
                if let [expr] = parenthesized.expr() {
                    let (expr, new_context) = Self::analyze_sqlite_scaler_expr(
                        context,
                        with_clause,
                        from_clause,
                        expr,
                        tokens,
                    )?;
                    context = new_context;
                    expr
                } else {
                    unsupported_sqlite_expr("row-value expr")
                }
            }
            Expr::Unary(expr_unary) => {
                let (expr, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_unary.expr(),
                    tokens,
                )?;
                context = new_context;

                match expr_unary.unary_operator() {
                    UnaryOperator::Positive => expr,
                    UnaryOperator::Negative => match expr {
                        Self::Const(ScalarConstExpr::Integer(value)) => {
                            Self::Const(ScalarConstExpr::Integer(-value))
                        }
                        Self::Const(ScalarConstExpr::Float(value)) => {
                            Self::Const(ScalarConstExpr::Float(format!("-{value}")))
                        }
                        _ => unsupported_sqlite_expr("unary negative expr"),
                    },
                    UnaryOperator::Not => {
                        Self::Boolean(BooleanExpr::new(BooleanExprBody::not(&expr)))
                    }
                    UnaryOperator::BitwiseNot => unsupported_sqlite_expr("bitwise not expr"),
                }
            }
            Expr::IsNull(expr_is_null) => {
                let (expr, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_is_null.expr(),
                    tokens,
                )?;
                context = new_context;
                Self::NullTest(NullTestExpr::new(NullTestType::IsNull, expr))
            }
            Expr::NotNull(expr_not_null) => {
                let (expr, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_not_null.expr(),
                    tokens,
                )?;
                context = new_context;
                Self::NullTest(NullTestExpr::new(NullTestType::IsNotNull, expr))
            }
            Expr::Binary(expr_binary) => {
                let (left, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_binary.left(),
                    tokens,
                )?;
                context = new_context;
                let (right, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_binary.right(),
                    tokens,
                )?;
                context = new_context;

                match expr_binary.operator() {
                    Operator::And => {
                        Self::Boolean(BooleanExpr::new(BooleanExprBody::and(&[left, right])))
                    }
                    Operator::Or => {
                        Self::Boolean(BooleanExpr::new(BooleanExprBody::or(&[left, right])))
                    }
                    _ => unsupported_sqlite_expr("binary expr"),
                }
            }
            Expr::Collate(expr_collate) => {
                let (expr, new_context) = Self::analyze_sqlite_scaler_expr(
                    context,
                    with_clause,
                    from_clause,
                    expr_collate.expr(),
                    tokens,
                )?;
                context = new_context;
                expr
            }
            Expr::Between(_) => unsupported_sqlite_expr("between expr"),
            Expr::Register(_) => unsupported_sqlite_expr("register expr"),
            Expr::Case(_) => unsupported_sqlite_expr("case expr"),
            Expr::Cast(_) => unsupported_sqlite_expr("cast expr"),
            Expr::DoublyQualified(_) => unsupported_sqlite_expr("doubly qualified expr"),
            Expr::Exists(_) => unsupported_sqlite_expr("exists expr"),
            Expr::FunctionCall(_) => unsupported_sqlite_expr("function call expr"),
            Expr::FunctionCallStar(_) => unsupported_sqlite_expr("function call star expr"),
            Expr::Id(_) => unsupported_sqlite_expr("id expr"),
            Expr::Column(_) => unsupported_sqlite_expr("column expr"),
            Expr::RowId(_) => unsupported_sqlite_expr("rowid expr"),
            Expr::InList(_) => unsupported_sqlite_expr("in-list expr"),
            Expr::InSelect(_) => unsupported_sqlite_expr("in-select expr"),
            Expr::InTable(_) => unsupported_sqlite_expr("in-table expr"),
            Expr::Like(_) => unsupported_sqlite_expr("like expr"),
            Expr::Name(_) => unsupported_sqlite_expr("name expr"),
            Expr::Qualified(_) => unsupported_sqlite_expr("qualified expr"),
            Expr::Raise(_) => unsupported_sqlite_expr("raise expr"),
            Expr::Subquery(_) => unsupported_sqlite_expr("subquery expr"),
            Expr::Variable(_) => unsupported_sqlite_expr("variable expr"),
            Expr::SubqueryResult(_) => unsupported_sqlite_expr("subquery result expr"),
            Expr::FieldAccess(_) => todo!(),
            Expr::Default => todo!(),
            Expr::Array(_) => todo!(),
            Expr::Subscript(_) => todo!(),
        };

        Ok((expr, context))
    }
}

/// analyze [`crate::syn::Node`] as scaler expression
#[deprecated = "use SemScalarExpr::analyze_scaler_expr"]
pub fn analyze_scaler_expr<TParseContext>(
    context: TParseContext,
    with_clause: &WithClause,
    from_clause: &FromClause,
    syn: crate::syn::Node,
    tokens: &IVec<ScanToken>,
) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    SemScalarExpr::analyze_scaler_expr(context, with_clause, from_clause, syn, tokens)
}
