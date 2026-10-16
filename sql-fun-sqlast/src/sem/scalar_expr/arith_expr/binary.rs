mod bin_cast_candidate;
mod impl_build_binary_op;
mod impl_build_op_any;

use self::bin_cast_candidate::BinaryCastCandidate;

use crate::sem::scalar_expr::arith_expr::{ArithExpr, UndefinedOperatorAllCallSite};
use crate::sem::{
    AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorDefinition, OperatorName,
    ParseContext, SemScalarExpr,
};

impl ArithExpr {
    /// create a instance
    pub fn build_op_all<TParseContext>(
        context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::build_op_all_resolved(context, name, lexpr, rexpr)
    }

    pub(crate) fn build_op_all_resolved<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let Some(lexpr) = lexpr else {
            AnalysisError::raise_unexpected_none("a_expr.lexpr")?
        };
        let Some(rexpr) = rexpr else {
            AnalysisError::raise_unexpected_none("a_expr.rexpr")?
        };

        if let Some(operator_def) = context.get_operator_op(name, true, true) {
            if lexpr.get_type().is_none() || rexpr.get_type().is_none() {
                return Ok((Self::deferred_all_operator(name, lexpr, rexpr), context));
            }
            let operator_def = operator_def.clone();

            let resolved_bin_op =
                Self::resolve_overload_binary(&mut context, operator_def, lexpr, rexpr)?;
            resolved_bin_op.into_all_call_site_expr(context, name)
        } else {
            UndefinedOperatorAllCallSite::undefined_operator_all_resolved(
                context, name, lexpr, rexpr,
            )
        }
    }
}

#[cfg(test)]
mod test_arith_expr_build_op_all {
    use crate::sem::scalar_expr::arith_expr::binary::ArithExpr;
    use crate::{
        sem::{
            BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
            OperatorName, SemScalarExpr, TypeReference, names::ValidOperatorName,
            scalar_expr::const_expr::ScalarConstExpr,
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_build_op_all(mut test_context: TestParseContext) -> TestResult {
        let name = OperatorName::Valid(ValidOperatorName::new("+"));
        let type_ref = TypeReference::from_master_type_name("int4");
        test_context.set_get_implicit_cast_result(
            &type_ref,
            &type_ref,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );
        let expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));
        let operator_def = BinaryOperatorDefinition::new(&type_ref, &type_ref, &type_ref, true);
        let operator_def = OperatorDefinition::Binary(vec![operator_def]);
        test_context.set_get_operator_op_result(&name, true, true, Some(&operator_def));

        ArithExpr::build_op_all_resolved(
            test_context,
            &name,
            Some(expr.clone()),
            Some(expr.clone()),
        )?;

        Ok(())
    }
}

pub(super) struct ResolvedBinaryOperator {
    operator_def: Option<BinaryOperatorDefinition>,
    lexpr: SemScalarExpr,
    rexpr: SemScalarExpr,
}

impl ResolvedBinaryOperator {
    const fn new(
        operator_def: Option<BinaryOperatorDefinition>,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Self {
        Self {
            operator_def,
            lexpr,
            rexpr,
        }
    }

    pub(super) fn into_call_site_expr<TParseContext>(
        self,
        context: TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        if let Some(operator) = self.operator_def {
            super::BinaryOpCallSite::from_operator_definition_resolved(
                context, name, operator, self.lexpr, self.rexpr,
            )
        } else {
            super::UndefinedOperatorCallSite::undefined_operator_resolved(
                context,
                name,
                Some(self.lexpr),
                Some(self.rexpr),
            )
        }
    }

    pub(super) fn into_any_call_site_expr<TParseContext>(
        self,
        context: TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        if let Some(operator) = self.operator_def {
            super::AnyBinaryCallSite::from_operator_definition_resolved(
                context, name, operator, self.lexpr, self.rexpr,
            )
        } else {
            super::UndefinedOperatorCallSite::undefined_operator_resolved(
                context,
                name,
                Some(self.lexpr),
                Some(self.rexpr),
            )
        }
    }

    pub(super) fn into_all_call_site_expr<TParseContext>(
        self,
        context: TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        if let Some(operator) = self.operator_def {
            super::AllBinaryCallSite::from_operator_definition_resolved(
                context, name, operator, self.lexpr, self.rexpr,
            )
        } else {
            super::UndefinedOperatorCallSite::undefined_operator_resolved(
                context,
                name,
                Some(self.lexpr),
                Some(self.rexpr),
            )
        }
    }
}

impl ArithExpr {
    pub(super) fn resolve_overload_binary<TParseContext>(
        context: &mut TParseContext,
        operator_def: OperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<ResolvedBinaryOperator, AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let OperatorDefinition::Binary(ref overloads) = operator_def else {
            AnalysisError::raise_unexpected_input("resolve_overload_binary with non binary op")?
        };

        if let Some(left_type) = lexpr.get_type()
            && let Some(right_type) = rexpr.get_type()
        {
            let candidates = BinaryCastCandidate::enumerate_from_context_resolved(
                context,
                overloads,
                &left_type,
                &right_type,
            );
            if candidates.is_empty() {
                context.report_problem(
                    AnalysisProblem::binary_operator_overload_resolution_failed(
                        &operator_def,
                        &left_type,
                        &right_type,
                    ),
                )?;
                return Ok(ResolvedBinaryOperator::new(None, lexpr, rexpr));
            }
            let filtered = BinaryCastCandidate::filter_best_candidate(candidates);
            if filtered.len() > 1 {
                context.report_problem(AnalysisProblem::binary_operator_overload_ambiguous(
                    &operator_def,
                    &left_type,
                    &right_type,
                ))?;
            }
            let candidate = &filtered[0];
            let (lexpr, rexpr) = candidate.wrap_implicit_cast(lexpr, rexpr);
            Ok(ResolvedBinaryOperator::new(
                Some(candidate.operator().clone()),
                lexpr,
                rexpr,
            ))
        } else {
            if lexpr.get_type().is_none() {
                context.report_problem(AnalysisProblem::expression_type_not_known(&lexpr))?;
            }
            if rexpr.get_type().is_none() {
                context.report_problem(AnalysisProblem::expression_type_not_known(&rexpr))?;
            }

            tracing::info!("argument type unknown");
            Ok(ResolvedBinaryOperator::new(None, lexpr, rexpr))
        }
    }
}
