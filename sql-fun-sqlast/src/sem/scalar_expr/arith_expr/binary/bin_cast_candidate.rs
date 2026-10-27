use crate::sem::overload_rank::OverloadRank;
use crate::sem::type_system::SimpleInferContext;
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
            let Some(instantiated) =
                Self::instantiate_array_overload(context, overload, left_type, right_type)
            else {
                continue;
            };
            if let Some(left_cast) =
                left_type.implicit_match_context(instantiated.left_arg_type(), context)
                && let Some(right_cast) =
                    right_type.implicit_match_context(instantiated.right_arg_type(), context)
            {
                let priority = Self::argument_rank(
                    left_cast,
                    overload.left_arg_type(),
                    instantiated.left_arg_type(),
                ) + Self::argument_rank(
                    right_cast,
                    overload.right_arg_type(),
                    instantiated.right_arg_type(),
                );

                candidates.push(Self::new(left_cast, right_cast, priority, instantiated));
            }
        }
        candidates
    }

    // A simple polymorphic array declaration shares one element type across
    // both operands. Instantiate a local signature, leaving catalog data intact.
    fn instantiate_array_overload(
        context: &impl ParseContext,
        overload: &BinaryOperatorDefinition,
        left: &TypeReference,
        right: &TypeReference,
    ) -> Option<BinaryOperatorDefinition> {
        let inputs = [
            (overload.left_arg_type(), left),
            (overload.right_arg_type(), right),
        ];
        if !inputs
            .into_iter()
            .any(|(declared, _)| !declared.is_dynamic() && declared.is_any_array())
        {
            return Some(overload.clone());
        }
        let mut inference = SimpleInferContext::default();
        for (declared, actual) in inputs {
            if !declared.is_dynamic() {
                inference.push_input_param(declared, actual, context).ok()?;
            }
        }
        let substitution = inference.get_inference_result().ok()?;
        let mut left = overload.left_arg_type().clone();
        let mut right = overload.right_arg_type().clone();
        let mut result = overload.get_return_type().clone();
        for ty in [&mut left, &mut right, &mut result] {
            if !ty.is_dynamic() && ty.is_simple_infer_arg() {
                substitution.apply(ty);
            }
        }
        Some(BinaryOperatorDefinition::new(
            &left,
            &right,
            &result,
            overload.is_strict(),
        ))
    }

    fn argument_rank(
        cast: CastContext,
        declared: &TypeReference,
        instantiated: &TypeReference,
    ) -> OverloadRank {
        // Substituting anyarray with an operand type establishes applicability,
        // but must not earn the exact-match credit of a concrete declaration.
        if declared == instantiated {
            OverloadRank::from(cast)
        } else {
            OverloadRank::default()
        }
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

    // ANY/ALL compare scalar elements, but retain the whole array operand.
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

#[cfg(test)]
mod tests {
    use super::BinaryCastCandidate;
    use crate::sem::{
        BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition, PgBuiltInType,
        TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    #[rstest::rstest]
    #[case::same_array("_oid", "_oid", true)]
    #[case::different_array("_oid", "_text", false)]
    #[case::numeric_array("_int4", "_int8", false)]
    #[case::scalar_left("oid", "_oid", false)]
    #[case::scalar_right("_oid", "oid", false)]
    fn array_inputs_share_one_element_type(
        #[case] left: &str,
        #[case] right: &str,
        #[case] applicable: bool,
    ) {
        let placeholder = TypeReference::from_master_type_name("anyarray");
        let element = TypeReference::from_master_type_name("anyelement");
        let declaration = BinaryOperatorDefinition::new(&placeholder, &placeholder, &element, true);
        let left = TypeReference::from_master_type_name(left);
        let right = TypeReference::from_master_type_name(right);
        let instantiated = BinaryCastCandidate::instantiate_array_overload(
            &TestParseContext::default(),
            &declaration,
            &left,
            &right,
        );
        assert_eq!(instantiated.is_some(), applicable);
        if let Some(instantiated) = instantiated {
            assert_eq!(instantiated.left_arg_type(), &left);
            assert_eq!(instantiated.right_arg_type(), &right);
            assert_eq!(instantiated.get_return_type(), &left.element_type());
            assert!(instantiated.is_strict());
        }
        assert_eq!(declaration.left_arg_type(), &placeholder);
        assert_eq!(declaration.right_arg_type(), &placeholder);
        assert_eq!(declaration.get_return_type(), &element);
    }

    #[rstest::rstest]
    fn concrete_array_overload_beats_polymorphic(#[values(false, true)] reversed: bool) {
        let mut context = TestParseContext::default();
        let array = PgBuiltInType::int4().array_type();
        context.set_get_implicit_cast_result(
            &array,
            &array,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        let placeholder = TypeReference::from_master_type_name("anyarray");
        let generic =
            BinaryOperatorDefinition::new(&placeholder, &placeholder, &PgBuiltInType::text(), true);
        let concrete = BinaryOperatorDefinition::new(&array, &array, &PgBuiltInType::bool(), true);
        let overloads = if reversed {
            [concrete.clone(), generic]
        } else {
            [generic, concrete.clone()]
        };
        let candidates = BinaryCastCandidate::enumerate_from_context_resolved(
            &context, &overloads, &array, &array,
        );
        assert_eq!(candidates.len(), 2);
        let best = BinaryCastCandidate::filter_best_candidate(candidates);
        assert_eq!(best.len(), 1);
        assert_eq!(best[0].operator(), &concrete);
    }

    #[test]
    fn tied_array_candidates_report_ambiguity() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let array = PgBuiltInType::int4().array_type();
        context.set_get_implicit_cast_result(
            &array,
            &array,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        let placeholder = TypeReference::from_master_type_name("anyarray");
        let first =
            BinaryOperatorDefinition::new(&placeholder, &placeholder, &PgBuiltInType::bool(), true);
        let second =
            BinaryOperatorDefinition::new(&placeholder, &placeholder, &PgBuiltInType::text(), true);
        let operator = OperatorDefinition::binary(&[first, second]);
        let best = BinaryCastCandidate::resolve(&mut context, &operator, &array, &array)?;
        assert_eq!(best.len(), 2);
        assert!(context.has_reported_problem("binary_operator_overload_ambiguous"));
        Ok(())
    }
}
