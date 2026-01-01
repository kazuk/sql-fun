use crate::{
    sem::{
        AnalysisError, ArithExpr, ArrayExpr, BooleanExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, FromClause, FuncCallExpr, IndirectionExpr, MinMaxExpr, NamedArgExpr,
        NullTestExpr, ParamRef, ParseContext, ScalarConstExpr, SemScalarExpr, SqlValueExpr,
        SubLinkExpr, TypeCastExpr, WithClause, scalar_expr::AnalyzeScalarExpr,
    },
    syn::Opt,
};

/// analyze [`crate::syn::Node`] as scaler expression
pub fn analyze_scaler_expr<TParseContext>(
    context: TParseContext,
    with_clause: &WithClause,
    from_clause: &FromClause,
    syn: crate::syn::Node,
) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    if syn.is_a_const() {
        if let Some(aconst) = syn.as_a_const().as_inner() {
            ScalarConstExpr::analyze_scalar_expr(context, with_clause, from_clause, aconst)
        } else {
            Ok((ScalarConstExpr::null(), context))
        }
    } else if let Some(col_ref) = syn.as_column_ref().as_inner() {
        ColumnReferenceExpr::analyze_scalar_expr(context, with_clause, from_clause, col_ref)
    } else if let Some(fun_call) = syn.as_func_call().as_inner() {
        FuncCallExpr::analyze_scalar_expr(context, with_clause, from_clause, fun_call)
    } else if let Some(a_expr) = syn.as_a_expr().as_inner() {
        ArithExpr::analyze_scalar_expr(context, with_clause, from_clause, a_expr)
    } else if let Some(cast) = syn.as_type_cast().as_inner() {
        TypeCastExpr::analyze_scalar_expr(context, with_clause, from_clause, cast)
    } else if let Some(case) = syn.as_case_expr().as_inner() {
        CaseExpr::analyze_scalar_expr(context, with_clause, from_clause, case)
    } else if let Some(bexpr) = syn.as_bool_expr().as_inner() {
        BooleanExpr::analyze_scalar_expr(context, with_clause, from_clause, bexpr)
    } else if let Some(nt) = syn.as_null_test().as_inner() {
        NullTestExpr::analyze_scalar_expr(context, with_clause, from_clause, nt)
    } else if let Some(sl) = syn.as_sub_link().as_inner() {
        SubLinkExpr::analyze_scalar_expr(context, with_clause, from_clause, sl)
    } else if let Some(ce) = syn.as_coalesce_expr().as_inner() {
        CoalesceExpr::analyze_scalar_expr(context, with_clause, from_clause, ce)
    } else if let Some(mme) = syn.as_min_max_expr().as_inner() {
        MinMaxExpr::analyze_scalar_expr(context, with_clause, from_clause, mme)
    } else if let Some(ce) = syn.as_collate_expr().as_inner() {
        CollateExpr::analyze_scalar_expr(context, with_clause, from_clause, ce)
    } else if let Some(svf) = syn.as_sqlvalue_function().as_inner() {
        SqlValueExpr::analyze_scalar_expr(context, with_clause, from_clause, svf)
    } else if let Some(pr) = syn.as_param_ref().as_inner() {
        ParamRef::analyze_scalar_expr(context, with_clause, from_clause, pr)
    } else if let Some(nae) = syn.as_named_arg_expr().as_inner() {
        NamedArgExpr::analyze_scalar_expr(context, with_clause, from_clause, nae)
    } else if let Some(ind) = syn.as_a_indirection().as_inner() {
        IndirectionExpr::analyze_scalar_expr(context, with_clause, from_clause, ind)
    } else if let Some(arr) = syn.as_a_array_expr().as_inner() {
        ArrayExpr::analyze_scalar_expr(context, with_clause, from_clause, arr)
    } else {
        AnalysisError::raise_unexpected_input(&format!(
            "analyze_scaler_expr unexpected node :{syn:?} "
        ))
    }
}
