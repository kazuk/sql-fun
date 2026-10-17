use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{AnalysisError, AnalysisProblem, SemScalarExpr, TypeReference};

use std::collections::HashSet;

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

pub(crate) struct CoalesceAnalysis {
    arg_exprs: Vec<SemScalarExpr>,
    expr_types: HashSet<TypeReference>,
    first_non_null: Option<SemScalarExpr>,
}

impl CoalesceAnalysis {
    pub(crate) fn new() -> Self {
        Self {
            arg_exprs: Vec::new(),
            expr_types: HashSet::new(),
            first_non_null: None,
        }
    }
    pub(crate) fn push<C: crate::sem::ParseContext>(
        &mut self,
        context: &mut C,
        arg_expr: &SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        if let Some(fnn) = &self.first_non_null {
            context.report_analysis_problem(AnalysisProblem::unreachable_coalesce_argument(
                arg_expr, fnn,
            ))?;
        } else if let Some(nn) = &arg_expr.is_not_null()
            && *nn
        {
            self.first_non_null = Some(arg_expr.clone());
        }

        if let Some(expr_type) = arg_expr.get_type() {
            let Some(type_ref) = context.canonicalize_type_ref(&expr_type) else {
                Err(AnalysisError::type_canonicalize_failed(&expr_type))?
            };
            self.expr_types.insert(type_ref);
        } else {
            context.report_analysis_problem(AnalysisProblem::unknown_type_in_coalesce_expr(
                arg_expr,
            ))?;
        }
        self.arg_exprs.push(arg_expr.clone());
        Ok(())
    }
    pub(crate) fn finish<C: crate::sem::ParseContext>(
        self,
        context: &mut C,
    ) -> Result<(SemScalarExpr, &mut C), AnalysisError> {
        if self.expr_types.len() == 1 {
            // ALL expressions returns same type no cast required
            let coalesce_type = self.expr_types.iter().next().cloned().unwrap();
            return Ok((
                SemScalarExpr::coalesce(&CoalesceExpr {
                    arg_exprs: self.arg_exprs,
                    coalesce_type: Some(coalesce_type),
                }),
                context,
            ));
        }

        let mut candidates: Option<HashSet<TypeReference>> = None;
        for expr_type in &self.expr_types {
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
                context.report_analysis_problem(AnalysisProblem::coalesce_expr_type_ambiguous(
                    &self.arg_exprs,
                    &final_candidates,
                    choose,
                ))?;
            }
            choose
        } else {
            context.report_analysis_problem(AnalysisProblem::coalesce_expr_type_unknown(
                &self.arg_exprs,
            ))?;
            None
        };

        Ok((
            SemScalarExpr::coalesce(&CoalesceExpr {
                arg_exprs: self.arg_exprs,
                coalesce_type: coalesce_type.cloned(),
            }),
            context,
        ))
    }
}

impl CoalesceExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for value in &mut self.arg_exprs {
            value.walk(visitor)?;
        }
        Ok(())
    }
}

impl CoalesceExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) {
        if self
            .arg_exprs
            .iter()
            .any(|value| value.needs_type_inference())
        {
            parameters.unify_coalesce_types(self);
        }
    }
}

impl CoalesceExpr {
    pub(crate) fn defer_untyped(arguments: &[SemScalarExpr]) -> Option<SemScalarExpr> {
        if arguments
            .iter()
            .all(|argument| argument.get_type().is_some())
        {
            return None;
        }
        Some(SemScalarExpr::coalesce(&Self {
            arg_exprs: arguments.to_vec(),
            coalesce_type: None,
        }))
    }
}
