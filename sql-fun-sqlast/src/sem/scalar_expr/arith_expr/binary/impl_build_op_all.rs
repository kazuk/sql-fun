use super::BinaryCastCandidate;

use crate::sem::{
    AnalysisError, MinMaxExpr, OperatorName, ParseContext, PgBuiltInType, SemScalarExpr,
    scalar_expr::arith_expr::AllBinaryCallSite,
    scalar_expr::arith_expr::UndefinedOperatorAllCallSite,
};

impl super::ArithExpr {
    /// create a instance
    pub fn build_op_all<TParseContext>(
        mut context: TParseContext,
        name: &OperatorName,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = Self::build_op_all_resolved(&mut context, name, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn build_op_all_resolved<'context, TParseContext>(
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
            return UndefinedOperatorAllCallSite::undefined_operator_all_resolved(
                context, name, lexpr, rexpr,
            );
        };
        let (Some(left_type), Some(array_type)) = (lexpr.get_type(), rexpr.get_type()) else {
            return Ok((Self::deferred_all_operator(name, lexpr, rexpr), context));
        };
        let array_type = MinMaxExpr::base_type(context, &array_type);
        let Some(element_type) = context.get_array_item_type(&array_type) else {
            rexpr.require_array_resolved(context)?;
            return UndefinedOperatorAllCallSite::undefined_operator_all_resolved(
                context, name, lexpr, rexpr,
            );
        };
        let candidates =
            BinaryCastCandidate::resolve(context, &operator_def, &left_type, &element_type)?;
        if candidates.len() != 1 {
            return UndefinedOperatorAllCallSite::undefined_operator_all_resolved(
                context, name, lexpr, rexpr,
            );
        }
        let candidate = &candidates[0];
        if candidate.operator().get_return_type() != &PgBuiltInType::bool() {
            return UndefinedOperatorAllCallSite::undefined_operator_all_resolved(
                context, name, lexpr, rexpr,
            );
        }
        let (left, right) = candidate.wrap_array_implicit_cast(lexpr, rexpr);
        AllBinaryCallSite::from_operator_definition_resolved(
            context,
            name,
            candidate.operator().clone(),
            left,
            right,
        )
    }
}

#[cfg(test)]
mod test_build_op_all {
    use crate::sem::scalar_expr::{ArrayExpr, ScalarConstExpr};
    use crate::sem::{
        ArithExpr, BinaryOperatorDefinition, CastContext, CastDefinition, OperatorDefinition,
        OperatorName, PgBuiltInType, SemScalarExpr, TypeDefinition, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    #[rstest::rstest]
    #[case::exact(false, false, false)]
    #[case::element_coercion(true, false, false)]
    #[case::left_coercion(false, true, false)]
    #[case::both_coercions(true, true, false)]
    #[case::ambiguous(false, false, true)]
    fn element_overloads_and_array_coercions(
        #[case] coercion: bool,
        #[case] left_coercion: bool,
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
        let left_target = if left_coercion {
            PgBuiltInType::int8()
        } else {
            source.clone()
        };
        if left_coercion {
            context.set_get_implicit_cast_result(
                &source,
                &left_target,
                Some(CastDefinition::new(CastContext::implicit())),
            );
        }
        let name = OperatorName::new("=");
        let operator =
            BinaryOperatorDefinition::new(&left_target, &target, &PgBuiltInType::bool(), true);
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
            ArithExpr::build_op_all(context, &name, Some(left.clone()), Some(right.clone()))?;
        if ambiguous {
            assert!(context.has_reported_problem("binary_operator_overload_ambiguous"));
            assert!(context.has_reported_problem("undefined_operator"));
            assert_eq!(value.get_type(), None);
            let encoded = ::serde_json::to_value(value)?;
            assert_eq!(
                encoded["Arith"]["UndefinedOperatorAll"]["rarg"],
                ::serde_json::to_value(right)?
            );
        } else {
            assert!(context.reported_problems().is_empty());
            assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
            assert_eq!(value.is_not_null(), None);
            let encoded = ::serde_json::to_value(value)?;
            let encoded_call = &encoded["Arith"]["OpAllBinary"];
            assert_eq!(
                encoded_call["operator_def"],
                ::serde_json::to_value(&definitions[0])?
            );
            let encoded_left = &encoded_call["left_arg"];
            if left_coercion {
                assert_eq!(
                    encoded_left["ImplicitCast"]["cast_target"],
                    ::serde_json::to_value(&left_target)?
                );
                assert_eq!(
                    encoded_left["ImplicitCast"]["value"],
                    ::serde_json::to_value(&left)?
                );
            } else {
                assert_eq!(encoded_left, &::serde_json::to_value(&left)?);
            }
            let encoded_right = &encoded_call["right_arg"];
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
    #[case::non_array(
        Some(PgBuiltInType::int4()),
        PgBuiltInType::bool(),
        false,
        "require_array"
    )]
    #[case::incompatible(
        Some(PgBuiltInType::text()),
        PgBuiltInType::bool(),
        true,
        "binary_operator_overload_resolution_failed"
    )]
    #[case::non_boolean(
        Some(PgBuiltInType::int4()),
        PgBuiltInType::int4(),
        true,
        "undefined_operator"
    )]
    #[case::missing(None, PgBuiltInType::bool(), true, "undefined_operator")]
    fn invalid_operands_keep_all_identity(
        #[case] right_type: Option<TypeReference>,
        #[case] result_type: TypeReference,
        #[case] array: bool,
        #[case] code: &str,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let source = PgBuiltInType::int4();
        context.setup_type(source.clone());
        let name = OperatorName::new("=");
        let operator = right_type.map(|right| {
            OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
                &source,
                &right,
                &result_type,
                true,
            )])
        });
        context.set_get_operator_op_result(&name, true, true, operator.as_ref());
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let right = if array {
            ArrayExpr::resolve(
                &mut context,
                std::slice::from_ref(&left),
                Some(&source.array_type()),
            )?
        } else {
            left.clone()
        };
        let (value, context) =
            ArithExpr::build_op_all(context, &name, Some(left.clone()), Some(right.clone()))?;
        assert!(context.has_reported_problem(code));
        assert_eq!(value.get_type(), None);
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["UndefinedOperatorAll"];
        assert_eq!(call["larg"], ::serde_json::to_value(left)?);
        assert_eq!(call["rarg"], ::serde_json::to_value(right)?);
        for problem in context.reported_problems() {
            if problem.code() == "undefined_operator" {
                assert_eq!(
                    ::serde_json::to_value(problem)?["UndefinedOperator"][0],
                    "AexprOpAll"
                );
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
        #[values(false, true)] left_coercion: bool,
        #[values(false, true)] nonnull: bool,
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
        let left_target = if left_coercion {
            PgBuiltInType::int8()
        } else {
            PgBuiltInType::int4()
        };
        if left_coercion {
            context.set_get_implicit_cast_result(
                &PgBuiltInType::int4(),
                &left_target,
                Some(CastDefinition::new(CastContext::implicit())),
            );
        }
        let operator =
            BinaryOperatorDefinition::new(&left_target, &target, &PgBuiltInType::bool(), true);
        let name = OperatorName::new("=");
        context.set_get_operator_op_result(
            &name,
            true,
            true,
            Some(&OperatorDefinition::binary(std::slice::from_ref(&operator))),
        );
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let source = if nonnull {
            ArrayExpr::resolve(
                &mut context,
                std::slice::from_ref(&left),
                Some(&PgBuiltInType::int4().array_type()),
            )?
        } else {
            ScalarConstExpr::null()
        };
        let right = SemScalarExpr::wrap_implicit_cast(
            &format!("public.{domain}").parse::<TypeReference>()?,
            &source,
        );
        let (value, context) =
            ArithExpr::build_op_all(context, &name, Some(left.clone()), Some(right.clone()))?;
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["OpAllBinary"];
        assert_eq!(call["operator_def"], ::serde_json::to_value(operator)?);
        if left_coercion {
            assert_eq!(
                call["left_arg"]["ImplicitCast"]["cast_target"],
                ::serde_json::to_value(left_target)?
            );
            assert_eq!(
                call["left_arg"]["ImplicitCast"]["value"],
                ::serde_json::to_value(left)?
            );
        } else {
            assert_eq!(call["left_arg"], ::serde_json::to_value(left)?);
        }
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
    #[case::direct("1 = ALL(NULL::amount_array)", "public.amount_array", None)]
    #[case::chain("1 = ALL(NULL::amount_array_chain)", "public.amount_array_chain", None)]
    #[case::nonnull(
        "1 = ALL(ARRAY[1]::amount_array_chain)",
        "public.amount_array_chain",
        None
    )]
    #[case::direct_nonnull("1 = ALL(ARRAY[1,1]::amount_array)", "public.amount_array", None)]
    #[case::deep_chain("1 = ALL(NULL::amount_array_deep)", "public.amount_array_deep", None)]
    #[case::ordinary("1 = ALL(NULL::integer[])", "pg_catalog.int4[]", None)]
    #[case::non_array("1 = ALL(NULL::amount)", "public.amount", Some("require_array"))]
    #[case::incompatible(
        "true = ALL(NULL::amount_array)",
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
            &encoded["Arith"]["UndefinedOperatorAll"]["rarg"]
        } else {
            &encoded["Arith"]["OpAllBinary"]["right_arg"]
        };
        let retained_right: SemScalarExpr = ::serde_json::from_value(retained_right.clone())?;
        assert_eq!(
            retained_right.get_type(),
            Some(right_type.parse::<TypeReference>()?)
        );
        if let Some(code) = failure {
            let mut codes = Vec::new();
            for problem in analyzed.context().problems() {
                codes.push(problem.code());
            }
            assert!(codes.contains(&code), "{:?}", analyzed.context().problems());
            assert!(codes.contains(&"undefined_operator"));
            assert_eq!(column.col_type(), &None);
            let call = &encoded["Arith"]["UndefinedOperatorAll"];
            assert!(!call["larg"].is_null());
            assert!(!call["rarg"].is_null());
        } else {
            assert!(
                analyzed.context().problems().is_empty(),
                "{:?}",
                analyzed.context().problems()
            );
            assert_eq!(column.col_type(), &Some(PgBuiltInType::bool()));
            let call = &encoded["Arith"]["OpAllBinary"];
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
                    domain.full_name(),
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
                "CREATE DOMAIN amount AS integer; CREATE DOMAIN amount_array AS integer[]; CREATE DOMAIN amount_array_chain AS amount_array; CREATE DOMAIN amount_array_deep AS amount_array_chain;",
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

    #[rstest::rstest]
    #[case::int2vector("int2vector", "int2")]
    #[case::oidvector("oidvector", "oid")]
    fn catalog_vectors_use_context_element_type(
        #[case] vector: &str,
        #[case] element: &str,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        let vector = TypeReference::from_master_type_name(vector);
        let element = TypeReference::from_master_type_name(element);
        context.set_array_element(&vector, &element);
        context.setup_type(element.clone());
        let name = OperatorName::new("=");
        let operator =
            BinaryOperatorDefinition::new(&element, &element, &PgBuiltInType::bool(), true);
        context.set_get_operator_op_result(
            &name,
            true,
            true,
            Some(&OperatorDefinition::binary(std::slice::from_ref(&operator))),
        );
        let left = SemScalarExpr::wrap_implicit_cast(&element, &ScalarConstExpr::null());
        let right = SemScalarExpr::wrap_implicit_cast(&vector, &ScalarConstExpr::null());
        let (value, context) =
            ArithExpr::build_op_all(context, &name, Some(left.clone()), Some(right.clone()))?;
        assert!(context.reported_problems().is_empty());
        assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["OpAllBinary"];
        assert_eq!(call["operator_def"], ::serde_json::to_value(operator)?);
        assert_eq!(call["left_arg"], ::serde_json::to_value(left)?);
        assert_eq!(call["right_arg"], ::serde_json::to_value(right)?);
        Ok(())
    }

    #[test]
    fn dynamic_array_keeps_element_operator() -> testresult::TestResult {
        let context = test_helpers::context();
        let element = PgBuiltInType::int4();
        let array = TypeReference::dynamic(&TypeDefinition::array(&element));
        let left = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let right = SemScalarExpr::wrap_implicit_cast(&array, &ScalarConstExpr::null());
        let (value, context) = ArithExpr::build_op_all(
            context,
            &OperatorName::new("="),
            Some(left.clone()),
            Some(right.clone()),
        )?;
        assert!(context.reported_problems().is_empty());
        assert_eq!(value.get_type(), Some(PgBuiltInType::bool()));
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["OpAllBinary"];
        assert_eq!(
            call["operator_def"],
            ::serde_json::to_value(BinaryOperatorDefinition::new(
                &element,
                &element,
                &PgBuiltInType::bool(),
                true
            ))?
        );
        assert_eq!(call["left_arg"], ::serde_json::to_value(left)?);
        assert_eq!(call["right_arg"], ::serde_json::to_value(right)?);
        Ok(())
    }

    #[test]
    fn unresolved_parameter_retains_deferred_all() -> testresult::TestResult {
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
        let left = SemScalarExpr::parameter(1, 0, &crate::sem::InputTypeConstraint::any());
        let right = SemScalarExpr::parameter(2, 1, &crate::sem::InputTypeConstraint::any());
        let (value, context) =
            ArithExpr::build_op_all(context, &name, Some(left.clone()), Some(right.clone()))?;
        assert!(context.reported_problems().is_empty());
        assert_eq!(value.get_type(), None);
        let encoded = ::serde_json::to_value(value)?;
        let call = &encoded["Arith"]["UndefinedOperatorAll"];
        assert_eq!(call["larg"], ::serde_json::to_value(left)?);
        assert_eq!(call["rarg"], ::serde_json::to_value(right)?);
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
        let dynamic = TypeReference::dynamic(&TypeDefinition::builtin(source.full_name(), None));
        let right = SemScalarExpr::wrap_implicit_cast(&dynamic, &left);
        let (value, context) = ArithExpr::build_op_all(context, &name, Some(left), Some(right))?;
        assert_eq!(value.get_type(), None);
        assert!(context.has_reported_problem("undefined_operator"));
        let encoded = ::serde_json::to_value(value)?;
        assert!(encoded["Arith"].get("UndefinedOperatorAll").is_some());
        Ok(())
    }
}
