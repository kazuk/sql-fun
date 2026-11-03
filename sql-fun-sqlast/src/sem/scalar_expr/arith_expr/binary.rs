mod bin_cast_candidate;
mod impl_build_binary_op;
mod impl_build_op_all;
mod impl_build_op_any;

use self::bin_cast_candidate::BinaryCastCandidate;

use crate::sem::scalar_expr::arith_expr::ArithExpr;
use crate::{
    names::OperatorName,
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, OperatorDefinition, SemScalarExpr,
    },
};

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

    pub(super) fn into_call_site_expr<'context, TParseContext>(
        self,
        context: &'context mut TParseContext,
        name: &OperatorName,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
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
}

impl ArithExpr {
    pub(super) fn resolve_overload_binary<TParseContext>(
        context: &mut TParseContext,
        name: &OperatorName,
        operator_def: OperatorDefinition,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> Result<ResolvedBinaryOperator, AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        if let Some(left_type) = lexpr.get_type()
            && let Some(right_type) = rexpr.get_type()
        {
            let filtered = BinaryCastCandidate::resolve(
                context,
                name,
                &operator_def,
                &left_type,
                &right_type,
            )?;
            if filtered.is_empty() {
                return Ok(ResolvedBinaryOperator::new(None, lexpr, rexpr));
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
                context
                    .report_analysis_problem(AnalysisProblem::expression_type_not_known(&lexpr))?;
            }
            if rexpr.get_type().is_none() {
                context
                    .report_analysis_problem(AnalysisProblem::expression_type_not_known(&rexpr))?;
            }

            tracing::info!("argument type unknown");
            Ok(ResolvedBinaryOperator::new(None, lexpr, rexpr))
        }
    }
}
