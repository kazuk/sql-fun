use crate::sem::overload_rank::OverloadRank;
use crate::sem::{
    AnalysisError, AnalysisProblem, BinaryOperatorDefinition, CastContext, OperatorDefinition,
    ParseContext, SemScalarExpr, TypeReference,
};

pub struct BinaryCastCandidate {
    left: CastContext,
    right: CastContext,
    priority: OverloadRank,
    operator: BinaryOperatorDefinition,
}

impl BinaryCastCandidate {
    const fn new(
        left: CastContext,
        right: CastContext,
        priority: OverloadRank,
        operator: BinaryOperatorDefinition,
    ) -> Self {
        Self {
            left,
            right,
            priority,
            operator,
        }
    }

    pub(crate) fn enumerate_from_context_resolved<TParseContext>(
        context: &TParseContext,
        overloads: &[BinaryOperatorDefinition],
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Vec<Self>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let mut candidates = Vec::new();
        for overload in overloads {
            if let Some(left_cast) =
                left_type.implicit_match_context(overload.left_arg_type(), context)
                && let Some(right_cast) =
                    right_type.implicit_match_context(overload.right_arg_type(), context)
            {
                let priority = OverloadRank::from(left_cast) + OverloadRank::from(right_cast);

                candidates.push(Self::new(left_cast, right_cast, priority, overload.clone()));
            }
        }
        candidates
    }

    pub(super) fn resolve(
        context: &mut impl ParseContext,
        operator: &OperatorDefinition,
        left: &TypeReference,
        right: &TypeReference,
    ) -> Result<Vec<Self>, AnalysisError> {
        let Some(overloads) = operator.binary_overloads() else {
            return AnalysisError::raise_unexpected_input("binary operator expected");
        };
        let candidates = Self::enumerate_from_context_resolved(context, overloads, left, right);
        if candidates.is_empty() {
            context.report_analysis_problem(
                AnalysisProblem::binary_operator_overload_resolution_failed(operator, left, right),
            )?;
            return Ok(candidates);
        }
        let candidates = Self::filter_best_candidate(candidates);
        if candidates.len() > 1 {
            context.report_analysis_problem(
                AnalysisProblem::binary_operator_overload_ambiguous(operator, left, right),
            )?;
        }
        Ok(candidates)
    }

    // ANY compares scalar elements, but the retained operand is the whole array.
    pub(super) fn wrap_array_implicit_cast(
        &self,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> (SemScalarExpr, SemScalarExpr) {
        let left = if self.left.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.left_arg_type(), &left)
        } else {
            left
        };
        let right = if self.right.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(&self.operator.right_arg_type().array_type(), &right)
        } else {
            right
        };
        (left, right)
    }

    pub fn wrap_implicit_cast(
        &self,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> (SemScalarExpr, SemScalarExpr) {
        let lexpr = if self.left.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.left_arg_type(), &lexpr)
        } else {
            lexpr
        };
        let rexpr = if self.right.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.right_arg_type(), &rexpr)
        } else {
            rexpr
        };
        (lexpr, rexpr)
    }

    pub fn filter_best_candidate(candidates: Vec<Self>) -> Vec<Self> {
        let priority = candidates
            .iter()
            .map(|v| v.priority)
            .max()
            .expect("get max priority");

        candidates
            .into_iter()
            .filter(|v| v.priority == priority)
            .collect()
    }

    pub const fn operator(&self) -> &BinaryOperatorDefinition {
        &self.operator
    }
}
