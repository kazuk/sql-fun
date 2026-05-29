use std::collections::HashSet;

use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, FromClause, ParseContext, SemScalarExpr, TypeReference,
        WithClause,
    },
    syn::{ListOpt, ScanToken},
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
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(args) = syn.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("CoalesceExpr.args")?
        };

        let mut arg_exprs = Vec::with_capacity(args.len());
        let mut expr_types = HashSet::new();
        let mut first_non_null: Option<SemScalarExpr> = None;

        for arg in args {
            let (arg_expr, new_context) =
                SemScalarExpr::analyze_scaler_expr(context, with_clause, from_clause, arg, tokens)?;
            context = new_context;

            if let Some(fnn) = &first_non_null {
                context.report_problem(AnalysisProblem::unreachable_coalesce_argument(
                    &arg_expr, fnn,
                ))?;
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
                context
                    .report_problem(AnalysisProblem::unknown_type_in_coalesce_expr(&arg_expr))?;
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

        let mut candidates: Option<HashSet<TypeReference>> = None;
        for expr_type in &expr_types {
            if let Some(can) = candidates {
                candidates = Some(
                    can.intersection(&context.get_implicit_castable(expr_type))
                        .cloned()
                        .collect(),
                );
            } else {
                candidates = Some(context.get_implicit_castable(expr_type));
            }
        }
        let mut final_candidates = HashSet::new();
        let coalesce_type = if let Some(ref candidates) = candidates
            && !candidates.is_empty()
        {
            for candidate1 in candidates {
                for candidate2 in candidates {
                    if candidate1 == candidate2 {
                        continue;
                    }

                    if let Some(cast) = context.get_implicit_cast(candidate1, candidate2) {
                        if cast.is_no_conversion() || cast.is_implicit_preferred() {
                            final_candidates.insert(candidate2.clone());
                        }
                    } else if let Some(cast) = context.get_implicit_cast(candidate2, candidate1) {
                        if cast.is_no_conversion() || cast.is_implicit_preferred() {
                            final_candidates.insert(candidate1.clone());
                        }
                    } else {
                        AnalysisError::raise_unexpected_none("cast candidate lookup")?;
                    }
                }
            }
            let choose = final_candidates.iter().next();
            if final_candidates.len() > 1 {
                context.report_problem(AnalysisProblem::coalesce_expr_type_ambiguous(
                    &arg_exprs,
                    &final_candidates,
                    choose,
                ))?;
            }
            choose
        } else {
            context.report_problem(AnalysisProblem::coalesce_expr_type_unknown(&arg_exprs))?;
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
