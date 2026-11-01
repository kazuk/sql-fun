use super::BinaryCastCandidate;

use crate::sem::{
    AnalysisError, MinMaxExpr, OperatorName, ParseContext, PgBuiltInType, SemScalarExpr,
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
        let array_type = MinMaxExpr::base_type(context, &array_type);
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

    #[rstest::rstest]
    #[case::direct("amount_array", false)]
    #[case::chain("amount_array_chain", false)]
    #[case::coercion("amount_array_chain", true)]
    #[case::array_of_domains("amount[]", false)]
    fn array_domains_use_element_operator_and_retain_source(
        #[case] domain: &str,
        #[case] coercion: bool,
    ) -> testresult::TestResult {
        let mut context = test_helpers::domains();
        let target = if domain == "amount[]" {
            "public.amount".parse::<TypeReference>()?
        } else if coercion {
            PgBuiltInType::int8()
        } else {
            PgBuiltInType::int4()
        };
        context.set_get_implicit_cast_result(
            &target,
            &target,
            Some(CastDefinition::new(CastContext::no_conversion())),
        );
        if coercion {
            context.set_get_implicit_cast_result(
                &PgBuiltInType::int4(),
                &target,
                Some(CastDefinition::new(CastContext::implicit())),
            );
        }
        let operator = BinaryOperatorDefinition::new(
            &PgBuiltInType::int4(),
            &target,
            &PgBuiltInType::bool(),
            true,
        );
        let name = OperatorName::new("=");
        context.set_get_operator_op_result(
            &name,
            true,
            true,
            Some(&OperatorDefinition::binary(std::slice::from_ref(&operator))),
        );
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let right = SemScalarExpr::wrap_implicit_cast(
            &format!("public.{domain}").parse::<TypeReference>()?,
            &ScalarConstExpr::null(),
        );
        let (value, context) =
            ArithExpr::build_op_any(context, &name, Some(left.clone()), Some(right.clone()))?;
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["OpAnyBinary"];
        assert_eq!(call["operator_def"], ::serde_json::to_value(operator)?);
        assert_eq!(call["left_arg"], ::serde_json::to_value(left)?);
        if coercion {
            assert_eq!(
                call["right_arg"]["ImplicitCast"]["cast_target"],
                ::serde_json::to_value(target.array_type())?
            );
            assert_eq!(
                call["right_arg"]["ImplicitCast"]["value"],
                ::serde_json::to_value(right)?
            );
        } else {
            assert_eq!(call["right_arg"], ::serde_json::to_value(right)?);
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::direct("1 = ANY(NULL::amount_array)", "public.amount_array", None)]
    #[case::chain("1 = ANY(NULL::amount_array_chain)", "public.amount_array_chain", None)]
    #[case::nonnull(
        "1 = ANY(ARRAY[1]::amount_array_chain)",
        "public.amount_array_chain",
        None
    )]
    #[case::ordinary("1 = ANY(NULL::integer[])", "pg_catalog.int4[]", None)]
    #[case::non_array("1 = ANY(NULL::amount)", "public.amount", Some("require_array"))]
    #[case::incompatible(
        "true = ANY(NULL::amount_array)",
        "public.amount_array",
        Some("binary_operator_overload_resolution_failed")
    )]
    fn ddl_domains_resolve_through_shared_context(
        #[case] expression: &str,
        #[case] right_type: &str,
        #[case] failure: Option<&str>,
    ) -> testresult::TestResult {
        let mut cache_context = crate::test_helpers::create_cache_context();
        let context = test_helpers::schema()?;
        let analyzed = crate::parse_statements(
            &format!("SELECT {expression}"),
            crate::ast::Dialect::postgres(),
        )
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?;
        let select = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = select.result_columns().get_column_at(0).unwrap();
        let value = column.expression().unwrap();
        assert!(value.operator_binding().unwrap().resolution().is_resolved());
        let encoded = ::serde_json::to_value(value)?;
        let retained_right = if failure.is_some() {
            &encoded["Arith"]["UndefinedOperatorAny"]["rarg"]
        } else {
            &encoded["Arith"]["OpAnyBinary"]["right_arg"]
        };
        let retained_right: SemScalarExpr = ::serde_json::from_value(retained_right.clone())?;
        assert_eq!(
            retained_right.get_type(),
            Some(right_type.parse::<TypeReference>()?)
        );
        if let Some(code) = failure {
            assert!(
                analyzed
                    .context()
                    .problems()
                    .iter()
                    .any(|problem| problem.code() == code),
                "{:?}",
                analyzed.context().problems()
            );
            assert!(
                analyzed
                    .context()
                    .problems()
                    .iter()
                    .any(|problem| problem.code() == "undefined_operator")
            );
            assert_eq!(column.col_type(), &None);
            let call = &encoded["Arith"]["UndefinedOperatorAny"];
            assert!(!call["larg"].is_null());
            assert!(!call["rarg"].is_null());
        } else {
            assert!(
                analyzed.context().problems().is_empty(),
                "{:?}",
                analyzed.context().problems()
            );
            assert_eq!(column.col_type(), &Some(PgBuiltInType::bool()));
            let call = &encoded["Arith"]["OpAnyBinary"];
            assert_eq!(
                call["operator_def"],
                ::serde_json::to_value(BinaryOperatorDefinition::new(
                    &PgBuiltInType::int4(),
                    &PgBuiltInType::int4(),
                    &PgBuiltInType::bool(),
                    true
                ))?
            );
            assert!(!call["left_arg"].is_null());
            assert!(!call["right_arg"].is_null());
        }
        Ok(())
    }

    mod test_helpers {
        use crate::{
            context::{BaseParseContext, SchemaFileContext},
            sem::{
                CreateDomain, ParseContext, PgBuiltInType, SchemaName, TypeDefinition,
                TypeReference,
            },
            test_helpers::TestParseContext,
        };

        pub(super) fn context() -> TestParseContext {
            let mut context =
                TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
            context.set_get_search_path_result(&[
                SchemaName::from("public"),
                SchemaName::from("pg_catalog"),
            ]);
            for ty in [
                PgBuiltInType::int4(),
                PgBuiltInType::int4().array_type(),
                PgBuiltInType::bool(),
            ] {
                context.setup_type(ty);
            }
            context.setup_binary("=", "int4", "int4", "bool", true);
            context
        }

        pub(super) fn domains() -> TestParseContext {
            let mut context = context();
            let amount = "public.amount".parse::<TypeReference>().unwrap();
            let array = "public.amount_array".parse::<TypeReference>().unwrap();
            let chain = "public.amount_array_chain"
                .parse::<TypeReference>()
                .unwrap();
            for (domain, base) in [
                (&amount, PgBuiltInType::int4()),
                (&array, PgBuiltInType::int4().array_type()),
                (&chain, array.clone()),
            ] {
                context.set_get_type_impl_result(
                    domain.full_name().expect("nominal type identity"),
                    &TypeDefinition::from(&CreateDomain::new(domain, &Some(base))),
                );
            }
            context
        }

        pub(super) fn schema() -> testresult::TestResult<SchemaFileContext> {
            let mut cache_context = crate::test_helpers::create_cache_context();
            let base = context();
            let args = base.get_args().clone();
            let metadata = base.metadata().clone();
            let mut context = SchemaFileContext::new(
                std::path::Path::new("query.sql"),
                &base.freeze(),
                &args,
                &metadata,
            );
            // Apply real DDL in the mutable schema catalog; no builtin metadata is loaded.
            for statement in crate::parse_statements(
                "CREATE DOMAIN amount AS integer; CREATE DOMAIN amount_array AS integer[]; CREATE DOMAIN amount_array_chain AS amount_array;",
                crate::ast::Dialect::postgres(),
            ) {
                context = statement?
                    .analyze(context, &mut cache_context)?
                    .into_context();
            }
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            Ok(context)
        }
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
        let dynamic = TypeReference::dynamic(&TypeDefinition::builtin(
            source.full_name().expect("nominal type identity"),
            None,
        ));
        let right = SemScalarExpr::wrap_implicit_cast(&dynamic, &left);
        let (value, context) = ArithExpr::build_op_any(context, &name, Some(left), Some(right))?;
        assert_eq!(value.get_type(), None);
        assert!(context.has_reported_problem("undefined_operator"));
        Ok(())
    }
}
