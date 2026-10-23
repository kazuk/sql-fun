use super::BinaryCastCandidate;

use crate::sem::{
    AnalysisError, OperatorName, ParseContext, PgBuiltInType, SemScalarExpr,
    scalar_expr::arith_expr::AnyBinaryCallSite,
    scalar_expr::arith_expr::UndefinedOperatorAnyCallSite,
};

impl super::ArithExpr {
    /// create a instance
    pub fn build_op_any<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::build_op_any_resolved(&mut context, name, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn build_op_any_resolved<'context, TParseContext>(
        context: &'context mut TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let Some(lexpr) = lexpr else {
            AnalysisError::raise_unexpected_none("a_expr.lexpr")?
        };
        let Some(rexpr) = rexpr else {
            AnalysisError::raise_unexpected_none("a_expr.rexpr")?
        };

        let Some(operator_def) = context.get_operator_op(name, true, true).cloned() else {
            return UndefinedOperatorAnyCallSite::undefined_operator_any_resolved(
                context, name, lexpr, rexpr,
            );
        };
        let (Some(left_type), Some(array_type)) = (lexpr.get_type(), rexpr.get_type()) else {
            return Ok((Self::deferred_any_operator(name, lexpr, rexpr), context));
        };
        if array_type.is_array() != Some(true) {
            rexpr.require_array_resolved(context)?;
            return UndefinedOperatorAnyCallSite::undefined_operator_any_resolved(
                context, name, lexpr, rexpr,
            );
        }
        let candidates = BinaryCastCandidate::resolve(
            context,
            &operator_def,
            &left_type,
            &array_type.element_type(),
        )?;
        if candidates.len() != 1 {
            return UndefinedOperatorAnyCallSite::undefined_operator_any_resolved(
                context, name, lexpr, rexpr,
            );
        }
        let candidate = &candidates[0];
        if candidate.operator().get_return_type() != &PgBuiltInType::bool() {
            return UndefinedOperatorAnyCallSite::undefined_operator_any_resolved(
                context, name, lexpr, rexpr,
            );
        }
        let (left, right) = candidate.wrap_array_implicit_cast(lexpr, rexpr);
        AnyBinaryCallSite::from_operator_definition_resolved(
            context,
            name,
            candidate.operator().clone(),
            left,
            right,
        )
    }
}

#[cfg(test)]
mod test_build_op_any {
    use crate::sem::scalar_expr::{ArrayExpr, ScalarConstExpr};
    use crate::sem::{
        ArithExpr, BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
        OperatorName, PgBuiltInType, SemScalarExpr, TypeDefinition, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    #[rstest::rstest]
    #[case::exact(false, false)]
    #[case::implicit(true, false)]
    #[case::ambiguous(false, true)]
    fn element_overloads_and_array_coercions(
        #[case] coercion: bool,
        #[case] ambiguous: bool,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let source = PgBuiltInType::int4();
        let target = if coercion || ambiguous {
            PgBuiltInType::int8()
        } else {
            source.clone()
        };
        context.setup_type(source.clone());
        context.set_get_implicit_cast_result(
            &source,
            &target,
            Some(CastDefinition::new(if coercion || ambiguous {
                CastContext::implicit()
            } else {
                CastContext::no_conversion()
            })),
        );
        let name = OperatorName::new("=");
        let operator =
            BinaryOperatorDefinition::new(&source, &target, &PgBuiltInType::bool(), true);
        let definitions = if ambiguous {
            let alternate = PgBuiltInType::numeric();
            context.set_get_implicit_cast_result(
                &source,
                &alternate,
                Some(CastDefinition::new(CastContext::implicit())),
            );
            vec![
                operator,
                BinaryOperatorDefinition::new(&source, &alternate, &PgBuiltInType::bool(), true),
            ]
        } else {
            vec![operator]
        };
        context.set_get_operator_op_result(
            &name,
            true,
            true,
            Some(&OperatorDefinition::binary(&definitions)),
        );
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let right = ArrayExpr::resolve(
            &mut context,
            std::slice::from_ref(&left),
            Some(&source.array_type()),
        )?;
        let (value, context) =
            ArithExpr::build_op_any(context, &name, Some(left), Some(right.clone()))?;
        if ambiguous {
            assert!(context.has_reported_problem("binary_operator_overload_ambiguous"));
            assert_eq!(value.get_type(), None);
            let encoded = ::serde_json::to_value(value)?;
            assert_eq!(
                encoded["Arith"]["UndefinedOperatorAny"]["rarg"],
                ::serde_json::to_value(right)?
            );
        } else {
            assert!(context.reported_problems().is_empty());
            assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
            let encoded = ::serde_json::to_value(value)?;
            let encoded_right = &encoded["Arith"]["OpAnyBinary"]["right_arg"];
            if coercion {
                assert_eq!(
                    encoded_right["ImplicitCast"]["cast_target"],
                    ::serde_json::to_value(target.array_type())?
                );
                assert_eq!(
                    encoded_right["ImplicitCast"]["value"],
                    ::serde_json::to_value(&right)?
                );
            } else {
                assert_eq!(encoded_right, &::serde_json::to_value(right)?);
            }
        }
        Ok(())
    }

    #[test]
    fn dynamic_non_array_is_rejected_without_element_projection() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let source = PgBuiltInType::int4();
        let name = OperatorName::new("=");
        let operator =
            BinaryOperatorDefinition::new(&source, &source, &PgBuiltInType::bool(), true);
        context.set_get_operator_op_result(
            &name,
            true,
            true,
            Some(&OperatorDefinition::binary(&[operator])),
        );
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let dynamic = TypeReference::dynamic(&TypeDefinition::builtin(source.full_name()));
        let right = SemScalarExpr::wrap_implicit_cast(&dynamic, &left);
        let (value, context) = ArithExpr::build_op_any(context, &name, Some(left), Some(right))?;
        assert_eq!(value.get_type(), None);
        assert!(context.has_reported_problem("undefined_operator"));
        Ok(())
    }
}
