use std::collections::HashSet;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, ParseContext, SemScalarExpr, TypeReference,
        WithClause, analyze_scaler_expr,
    },
    syn::ListOpt,
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

/// COALASCE expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CoalesceExpr {
    arg_exprs: Vec<SemScalarExpr>,
    coalesce_type: Option<TypeReference>,
}

impl SemScalarExprNode for CoalesceExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.coalesce_type.clone()
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::CoalesceExpr> for CoalesceExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::CoalesceExpr,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(args) = syn.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("CoalesceExpr.args")?
        };

        let mut arg_exprs = Vec::with_capacity(args.len());
        let mut expr_types = HashSet::new();
        let mut first_non_null: Option<SemScalarExpr> = None;

        for arg in args {
            let (arg_expr, new_context) =
                analyze_scaler_expr(context, with_clause, from_clause, arg)?;
            context = new_context;

            if let Some(fnn) = &first_non_null {
                context.report_problem(AnalysisProblem::unreachable_coalesce_argument(
                    &arg_expr, fnn,
                ))
            } else if let Some(nn) = &arg_expr.is_not_null()
                && *nn
            {
                first_non_null = Some(arg_expr.clone());
            }

            if let Some(expr_type) = arg_expr.get_type() {
                let Some(type_ref) = context.canonicalize_type_ref(&expr_type) else {
                    Err(AnalysisError::type_canonicalize_failed(&expr_type))?
                };
                expr_types.insert(type_ref);
            } else {
                context.report_problem(AnalysisProblem::unknown_type_in_coalesce_expr(&arg_expr));
            }
            arg_exprs.push(arg_expr);
        }

        if expr_types.len() == 1 {
            // ALL expressions returns same type no cast required
            let coalesce_type = expr_types.iter().next().cloned().unwrap();
            return Ok((
                SemScalarExpr::Coalesce(Self {
                    arg_exprs,
                    coalesce_type: Some(coalesce_type),
                }),
                context,
            ));
        }

        let mut candiates: Option<HashSet<TypeReference>> = None;
        for expr_type in &expr_types {
            if let Some(can) = candiates {
                candiates = Some(
                    can.intersection(&context.get_implicit_castable(expr_type))
                        .cloned()
                        .collect(),
                );
            } else {
                candiates = Some(context.get_implicit_castable(expr_type));
            }
        }
        let mut final_candiates = HashSet::new();
        let coalesce_type = if let Some(ref candiates) = candiates
            && !candiates.is_empty()
        {
            for candiate1 in candiates {
                for candiate2 in candiates {
                    if candiate1 == candiate2 {
                        continue;
                    }

                    if let Some(cast) = context.get_implicit_cast(candiate1, candiate2) {
                        if cast.is_no_coversion() || cast.is_implicit_preferred() {
                            final_candiates.insert(candiate2.clone());
                        }
                    } else if let Some(cast) = context.get_implicit_cast(candiate2, candiate1) {
                        if cast.is_no_coversion() || cast.is_implicit_preferred() {
                            final_candiates.insert(candiate1.clone());
                        }
                    } else {
                        AnalysisError::raise_unexpected_none("cast candiate lookup")?
                    };
                }
            }
            let choose = final_candiates.iter().next();
            if final_candiates.len() > 1 {
                context.report_problem(AnalysisProblem::coalesce_expr_type_ambiguous(
                    &arg_exprs,
                    &final_candiates,
                    choose,
                ));
            }
            choose
        } else {
            context.report_problem(AnalysisProblem::coalesce_expr_type_unknown(&arg_exprs));
            None
        };

        Ok((
            SemScalarExpr::Coalesce(Self {
                arg_exprs,
                coalesce_type: coalesce_type.cloned(),
            }),
            context,
        ))
    }
}
