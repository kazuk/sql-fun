use crate::{
    StringSpan,
    context::ParseContext as ContextParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, FunctionModifiers, FunctionOverloadCollection,
        FunctionParam, InputTypeConstraint, OverloadVariant, ParseContext, QueryNameResolution,
        SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        problem::CallArgumentProblem,
        scalar_expr::{
            SemScalarExprNode,
            func_call::{FuncCallArgs, FuncCallExpr},
        },
        type_system::ArgumentBindingCollection,
    },
};

use ::sql_fun_core::IVec;

impl FuncCallExpr {
    pub(crate) fn resolve_supplied(
        context: &mut impl ParseContext,
        name: &FullName,
        arguments: &[SemScalarExpr],
        argument_spans: &[Option<StringSpan>],
        span: &Option<StringSpan>,
        explicit_variadic: bool,
        window: bool,
    ) -> Result<Self, AnalysisError> {
        let mut invalid = Self::validate_supplied_notation(context, arguments, argument_spans)?;
        let args: Vec<_> = arguments
            .iter()
            .map(|argument| FuncCallArgs::new(argument, &None))
            .collect();
        let candidates = context.get_function_by_name(name);
        if !invalid
            && arguments
                .iter()
                .any(|argument| argument.as_named_argument().is_some())
            && !window
            && context.is_aggregate_impl(&context.resolve_function_name_with_search_path(name))
        {
            context.report_analysis_problem(
                AnalysisProblem::function_overload_resolution_failed(
                    name,
                    &ArgumentBindingCollection::from_expr_list(arguments),
                ),
            )?;
            invalid = true;
        }
        let found = candidates.is_some();
        if !found {
            context.report_analysis_problem(AnalysisProblem::function_not_found(
                name,
                &span.unwrap_or_else(|| StringSpan::new(0, 0)),
            ))?;
        }
        let mut call = Self::new(name, &None, &args.into());
        call.set_name_resolution(if found {
            QueryNameResolution::function(&context.resolve_function_name_with_search_path(name))
        } else {
            QueryNameResolution::not_found()
        });
        let mut modifiers = FunctionModifiers::default();
        modifiers.set_flags(false, false, explicit_variadic, false);
        call.set_modifiers(modifiers);
        if !invalid && let Some(candidates) = &candidates {
            let types = ArgumentBindingCollection::from_expr_list(arguments)
                .with_explicit_variadic(explicit_variadic);
            if let Some(problem) = candidates.binding_problem(&types) {
                context.report_analysis_problem(AnalysisProblem::call_argument(problem))?;
                invalid = true;
            } else if !arguments.iter().any(SemScalarExpr::requires_argument_type) {
                if let Some(overload) = candidates.resolve_from_catalog(context, &types) {
                    let bound = Self::cast_call_operands(
                        context,
                        name,
                        &arguments.to_vec().into(),
                        &overload,
                        explicit_variadic,
                    )?;
                    call.retain_signature(&overload, &bound);
                } else {
                    context.report_analysis_problem(
                        AnalysisProblem::function_overload_resolution_failed(name, &types),
                    )?;
                }
            }
        }
        call.set_candidates(if invalid {
            Some(Vec::new())
        } else {
            candidates.map(FunctionOverloadCollection::into_snapshot)
        });
        Ok(call)
    }
}

#[cfg(test)]
mod test_analyze_scalar_expr_reports_missing_function {
    use crate::{
        names::SchemaName,
        sem::{
            FromClause, FunctionParam, OverloadVariant, PgBuiltInType, ScalarAnalysisContext,
            SemScalarExpr, WithClause,
        },
        syn::{Node, NodeInner, NodeList},
        test_helpers::{
            SynBuilder, TestParseContext, function_syntax::func_name_tokens, test_context,
        },
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_scalar_expr_reports_missing_function(
        mut test_context: TestParseContext,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);

        let builder = SynBuilder::new();
        let arg_expr = Node::from(NodeInner::AConst(builder.const_int4(1)));
        let args = NodeList::from(vec![arg_expr]);
        let func_name_node = builder.as_string_node(builder.string("missing_func"));
        let funcname = NodeList::from(vec![func_name_node]);
        let func_call = builder.func_call(funcname, args);

        let tokens = func_name_tokens();
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let (expr, context) = SemScalarExpr::analyze_scaler_expr(
            test_context,
            &ScalarAnalysisContext::new(&with_clause, &from_clause),
            Node::from(NodeInner::from(func_call)),
            &tokens,
        )?;

        assert_eq!(1, context.reported_problem_count());
        let SemScalarExpr::FuncCall(func_call_expr) = expr else {
            panic!("expected func call expression");
        };
        assert!(func_call_expr.overload().is_none());
        assert_eq!(1, func_call_expr.args().len());
        assert!(matches!(
            func_call_expr.args()[0].arg_expr(),
            SemScalarExpr::Const(_)
        ));
        assert!(func_call_expr.args()[0].arg_definition().is_none());
        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_scalar_expr_resolves_overload(
        mut test_context: TestParseContext,
    ) -> TestResult {
        test_context.set_get_search_path_result(&[SchemaName::from("public")]);
        let int4 = PgBuiltInType::int4();
        test_context.setup_type(int4.clone());

        let params = vec![FunctionParam::new_input_param(
            &Some("arg".to_string()),
            &Some(int4.clone()),
            &None,
        )];
        let overload = OverloadVariant::new(&Some(int4.clone()), &params, false, false);
        test_context.setup_function("test_func", std::slice::from_ref(&overload));

        let builder = SynBuilder::new();
        let arg_expr = Node::from(NodeInner::AConst(builder.const_int4(1)));
        let args = NodeList::from(vec![arg_expr]);
        let func_name_node = builder.as_string_node(builder.string("test_func"));
        let funcname = NodeList::from(vec![func_name_node]);
        let func_call = builder.func_call(funcname, args);

        let tokens = func_name_tokens();
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let (expr, context) = SemScalarExpr::analyze_scaler_expr(
            test_context,
            &ScalarAnalysisContext::new(&with_clause, &from_clause),
            Node::from(NodeInner::from(func_call)),
            &tokens,
        )?;

        assert_eq!(0, context.reported_problem_count());
        let SemScalarExpr::FuncCall(func_call_expr) = expr else {
            panic!("expected func call expression");
        };
        assert_eq!(Some(&overload), func_call_expr.overload().as_ref());
        assert!(func_call_expr.args()[0].arg_definition().is_some());
        assert!(matches!(
            func_call_expr.args()[0].arg_expr(),
            SemScalarExpr::Const(_)
        ));
        Ok(())
    }
}

#[cfg(test)]
mod test_supplied_argument_binding {
    use crate::{
        names::FullName,
        sem::{PgBuiltInType, QueryNameResolution, ScalarConstExpr, SemScalarExpr},
        test_helpers::{
            call_arguments::{call, resolve},
            query::expression,
        },
    };

    #[rstest::rstest]
    #[case::reordered("SELECT f(c => 9, a => 1)", &["c", "a"], &[9, 1])]
    #[case::old_notation("SELECT f(c := 9, a := 1)", &["c", "a"], &[9, 1])]
    #[case::mixed("SELECT f(1, c => 9)", &["a", "c"], &[1, 9])]
    #[case::folded("SELECT f(A => 1)", &["a"], &[1])]
    fn binds_written_arguments_without_inserting_defaults(
        #[case] sql: &str,
        #[case] formals: &[&str],
        #[case] values: &[i32],
    ) {
        let (query, context) = resolve(sql, false);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let call = call(&query);
        assert!(!call.signature_is_deferred());
        assert!(call.overload().is_some());
        assert_eq!(call.args().len(), formals.len());
        assert_eq!(
            call.name_resolution(),
            &QueryNameResolution::function(&FullName::with_schema("public", "f"))
        );
        for (position, formal) in formals.iter().enumerate() {
            let argument = &call.args()[position];
            assert_eq!(argument.position(), position);
            assert_eq!(
                argument
                    .arg_definition()
                    .as_ref()
                    .unwrap()
                    .name()
                    .map(String::as_str),
                Some(*formal)
            );
            assert_eq!(
                argument.value(),
                &SemScalarExpr::literal(&ScalarConstExpr::new_integer(values[position]))
            );
            if argument.supplied_name().is_some() {
                assert!(argument.source_span().is_some());
            }
        }
        let original = ::serde_json::to_value(expression(&query)).unwrap();
        let restored: SemScalarExpr = ::serde_json::from_value(original.clone()).unwrap();
        assert_eq!(::serde_json::to_value(restored).unwrap(), original);
        // Omitted defaults do not prove non-nullness for a strict function.
        assert_eq!(expression(&query).is_not_null(), Some(false));
    }

    #[rstest::rstest]
    #[case::duplicate("SELECT f(a => 1, a => 2)", "call_argument_duplicate_name")]
    #[case::folded_duplicate("SELECT f(A => 1, a => 2)", "call_argument_duplicate_name")]
    #[case::placement("SELECT f(a => 1, 2)", "call_argument_positional_after_named")]
    #[case::unknown_name("SELECT f(z => 1)", "call_argument_unknown_name")]
    #[case::quoted_name(r#"SELECT f("A" => 1)"#, "call_argument_unknown_name")]
    #[case::assigned_twice("SELECT f(1, a => 2)", "call_argument_assigned_twice")]
    #[case::missing_required("SELECT f(c => 1)", "call_argument_missing_required")]
    #[case::wrong_type("SELECT f(a => TRUE)", "function_overload_resolution_failed")]
    #[case::missing_function("SELECT missing(a => 1)", "function_not_found")]
    fn failed_binding_retains_arguments_and_lookup(#[case] sql: &str, #[case] code: &str) {
        let (query, context) = resolve(sql, false);
        assert_eq!(
            context.reported_problem_count(),
            1,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(context.reported_problems()[0].code(), code);
        let call = call(&query);
        assert!(call.overload().is_none());
        assert!(!call.args().is_empty());
        assert_eq!(
            call.name_resolution().is_resolved(),
            code != "function_not_found"
        );
    }

    #[rstest::rstest]
    #[case::positional("SELECT v(VARIADIC ARRAY[1,2])", None)]
    #[case::named("SELECT v(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
    #[case::ordinary_array("SELECT arr(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
    fn explicit_variadic_binds_an_array_identity(#[case] sql: &str, #[case] name: Option<&str>) {
        let (query, context) = resolve(sql, true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let call = call(&query);
        assert!(call.modifiers().variadic());
        assert!(call.overload().is_some());
        assert_eq!(call.args().len(), 1);
        assert_eq!(call.args()[0].supplied_name(), name);
        assert_eq!(
            call.args()[0].arg_definition().as_ref().unwrap().get_type(),
            Some(&PgBuiltInType::int4().array_type())
        );
    }

    #[rstest::rstest]
    #[case::aggregate("SELECT named_sum(x=>1)", false)]
    #[case::window("SELECT named_sum(x=>1) OVER ()", true)]
    fn aggregate_notation_has_window_specific_legality(#[case] sql: &str, #[case] valid: bool) {
        // An explicit catalog isolates call legality from CREATE AGGREGATE parsing.
        let (query, context) = resolve(sql, false);
        assert_eq!(context.reported_problem_count(), usize::from(!valid));
        assert_eq!(call(&query).overload().is_some(), valid);
        if !valid {
            assert_eq!(
                context.reported_problems()[0].code(),
                "function_overload_resolution_failed"
            );
        }
    }
}

#[cfg(test)]
mod test_named_arguments_retain_typed_rows {
    use crate::{
        names::FullName,
        sem::{PgBuiltInType, TypeReference},
        test_helpers::{
            query::expression, query_whole_rows as test_helpers, query_whole_rows::catalog_query,
        },
    };

    #[test]
    fn named_arguments_retain_typed_rows() {
        let (query, context) = catalog_query(
            "SELECT accept_rows(left_row => a.*, right_row => t.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t",
        );
        assert_eq!(context.reported_problem_count(), 0);
        let call = expression(&query).as_function_call().unwrap();
        let row = call.args()[0].value().as_whole_row().unwrap();
        assert_eq!(call.args()[0].supplied_name(), Some("left_row"));
        assert_eq!(row.alias().unwrap().as_str(), "a");
        assert_eq!(
            call.args()[0].value().get_type(),
            Some(TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "pg_attribute"),
                false
            ))
        );
    }

    #[test]
    fn wrong_nominal_types_do_not_select_the_signature() {
        let (query, context) = catalog_query(
            "SELECT accept_rows(t.*, a.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t",
        );
        assert!(
            expression(&query)
                .as_function_call()
                .unwrap()
                .overload()
                .is_none()
        );
        test_helpers::assert_problem(&context, "function_overload_resolution_failed");
        assert_eq!(
            expression(&query).as_function_call().unwrap().args()[0]
                .value()
                .as_whole_row()
                .unwrap()
                .alias()
                .unwrap()
                .as_str(),
            "t"
        );
    }

    #[rstest::rstest]
    #[case::derived("SELECT record_arg(x.*) FROM (SELECT 1 AS n) x")]
    #[case::cte("WITH x AS (SELECT 1 AS n) SELECT record_arg(x.*) FROM x")]
    #[case::nested("SELECT identity(record_arg(x.*)) FROM (SELECT 1 AS n) x")]
    fn derived_rows_bind_record_arguments(#[case] sql: &str) {
        let (query, context) = catalog_query(sql);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
    }

    #[test]
    fn nominal_function_boundary_retains_argument_binding() {
        let (query, context) = test_helpers::alias_query("SELECT identity_v(r.*) FROM v AS r(x)");
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let call = expression(&query).as_function_call().unwrap();
        assert!(call.overload().is_some());
        let nominal =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
        assert_eq!(expression(&query).get_type(), Some(nominal.clone()));
        let argument = &call.args()[0];
        assert_eq!(argument.value().get_type(), Some(nominal.clone()));
        assert_eq!(
            argument.arg_definition().as_ref().unwrap().get_type(),
            Some(&nominal)
        );
        let row = argument.value().as_whole_row().unwrap();
        assert_eq!(row.alias().unwrap().as_str(), "r");
        assert_eq!(row.relation(), Some(&FullName::with_schema("public", "v")));
        assert_eq!(row.levels_up(), Some(0));
        assert!(row.failure_code().is_none());
        test_helpers::assert_catalog_view_field(&context);
    }
}

impl FuncCallExpr {
    fn validate_supplied_notation(
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
        argument_spans: &[Option<StringSpan>],
    ) -> Result<bool, AnalysisError> {
        let mut names = std::collections::HashSet::new();
        let mut invalid = false;
        for (position, argument) in arguments.iter().enumerate() {
            if let Some(named) = argument.as_named_argument() {
                if !names.insert(named.name()) {
                    invalid = true;
                    context.report_analysis_problem(AnalysisProblem::call_argument(
                        CallArgumentProblem::duplicate_name(
                            named.name(),
                            &named.source_span().map(Into::into),
                        ),
                    ))?;
                }
            } else if !names.is_empty() {
                invalid = true;
                context.report_analysis_problem(AnalysisProblem::call_argument(
                    CallArgumentProblem::positional_after_named(
                        position,
                        &argument_spans
                            .get(position)
                            .copied()
                            .flatten()
                            .as_ref()
                            .map(Into::into),
                    ),
                ))?;
            }
        }
        Ok(invalid)
    }

    fn cast_operand_expression<TParseContext>(
        context: &mut TParseContext,
        func_name: &FullName,
        arg_exprs: &IVec<SemScalarExpr>,
        overload: &OverloadVariant,
    ) -> Result<IVec<FuncCallArgs>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::cast_call_operands(context, func_name, arg_exprs, overload, false)
    }

    pub(super) fn cast_call_operands(
        context: &mut impl ParseContext,
        func_name: &FullName,
        arg_exprs: &IVec<SemScalarExpr>,
        overload: &OverloadVariant,
        explicit_variadic: bool,
    ) -> Result<IVec<FuncCallArgs>, AnalysisError> {
        let types = ArgumentBindingCollection::from_expr_list(arg_exprs)
            .with_explicit_variadic(explicit_variadic);
        let supplied = if types.has_call_notation() {
            types.supplied_parameters(overload).ok()
        } else {
            None
        };
        let mut args = Vec::new();
        for (index, arg) in arg_exprs.iter().enumerate() {
            let arg_def = if let Some(parameters) = &supplied {
                parameters.get(index).copied()
            } else {
                overload.call_argument(index)
            };
            let arg =
                Self::cast_call_value(context, func_name, index, arg, arg_def, explicit_variadic)?;

            args.push(FuncCallArgs::new(&arg, &arg_def.cloned()));
        }
        Ok(args.into())
    }
    fn cast_call_value(
        context: &mut impl ParseContext,
        func_name: &FullName,
        index: usize,
        argument: &SemScalarExpr,
        argument_definition: Option<&FunctionParam>,
        explicit_variadic: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut arg = argument.clone();
        if let Some(arg_definition) = argument_definition
            && !arg_definition.accepts_any_type()
            && let Some(arg_def_type) = if explicit_variadic {
                arg_definition.get_type().cloned()
            } else {
                arg_definition.call_argument_type()
            }
            && let Some(arg_val_type) = arg.get_type()
        {
            let Some(t) = arg_def_type.definition_from_catalog(context) else {
                AnalysisError::raise_unexpected_input(&format!(
                    "type {arg_def_type} not found in context"
                ))?
            };
            if let Some(cast) = arg_val_type.implicit_match_context(&arg_def_type, context) {
                if !cast.is_no_conversion() && arg_val_type != arg_def_type {
                    arg = if let Some(named) = arg.as_named_argument() {
                        SemScalarExpr::named_argument(
                            named.name(),
                            &SemScalarExpr::wrap_implicit_cast(&arg_def_type, named.value()),
                            &named.source_span().copied(),
                        )
                    } else {
                        SemScalarExpr::wrap_implicit_cast(&arg_def_type, &arg)
                    };
                }
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::function_arg_implicit_cast_not_found(
                        func_name, index, t, &arg,
                    ),
                )?;
            }
        }
        Ok(arg)
    }
}

#[cfg(test)]
mod test_func_call_expr_cast_operand_expression {
    use crate::{
        names::FullName,
        sem::{
            CastContext, CastDefinition, FunctionParam, OverloadVariant, PgBuiltInType,
            ScalarConstExpr, SemScalarExpr, scalar_expr::func_call::FuncCallExpr,
        },
        test_helpers::{TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_cast_operand_expression_inserts_implicit_cast(
        mut test_context: TestParseContext,
    ) -> TestResult {
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.setup_type(int4.clone());
        test_context.setup_type(text.clone());
        test_context.set_get_implicit_cast_result(
            &int4,
            &text,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let func_name = FullName::with_schema("public", "test_func");
        let arg_exprs: IVec<SemScalarExpr> =
            vec![SemScalarExpr::new_const(ScalarConstExpr::new_integer(1))].into();
        let params = vec![FunctionParam::new_input_param(
            &Some("arg".to_string()),
            &Some(text.clone()),
            &None,
        )];
        let overload = OverloadVariant::new(&Some(text), &params, false, false);

        let args = FuncCallExpr::cast_operand_expression(
            &mut test_context,
            &func_name,
            &arg_exprs,
            &overload,
        )?;

        assert!(matches!(args[0].arg_expr(), SemScalarExpr::ImplicitCast(_)));
        Ok(())
    }

    #[rstest::rstest]
    fn test_cast_operand_expression_reports_missing_cast(
        mut test_context: TestParseContext,
    ) -> TestResult {
        let int4 = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        test_context.setup_type(int4.clone());
        test_context.setup_type(text.clone());

        let func_name = FullName::with_schema("public", "test_func");
        let arg_exprs: IVec<SemScalarExpr> =
            vec![SemScalarExpr::new_const(ScalarConstExpr::new_integer(1))].into();
        let params = vec![FunctionParam::new_input_param(
            &Some("arg".to_string()),
            &Some(text),
            &None,
        )];
        let overload = OverloadVariant::new(&None, &params, false, false);

        let args = FuncCallExpr::cast_operand_expression(
            &mut test_context,
            &func_name,
            &arg_exprs,
            &overload,
        )?;

        assert!(matches!(args[0].arg_expr(), SemScalarExpr::Const(_)));
        assert_eq!(1, test_context.reported_problem_count());
        Ok(())
    }
}

impl FuncCallExpr {
    pub(crate) fn resolve_common<TContext: ParseContext>(
        context: &mut TContext,
        name: &FullName,
        arguments: &[SemScalarExpr],
        span: &StringSpan,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let resolved_name = context.resolve_function_name_with_search_path(name);
        let candidates = context.get_function_by_name(name);
        let found = candidates.is_some();
        let mut expression =
            Self::resolve_binding(context, name, arguments, span, candidates.as_ref())?;
        expression.set_function_name_resolution(if found {
            QueryNameResolution::function(&resolved_name)
        } else {
            QueryNameResolution::not_found()
        });
        if context.parameter_inference_needed() {
            expression
                .set_function_candidates(candidates.map(FunctionOverloadCollection::into_snapshot));
        }
        Ok(expression)
    }
    fn resolve_binding<TContext: ParseContext>(
        context: &mut TContext,
        name: &FullName,
        arguments: &[SemScalarExpr],
        span: &StringSpan,
        overloads: Option<&FunctionOverloadCollection>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arg_types = ArgumentBindingCollection::from_expr_list(arguments);
        if let Some(overloads) = overloads {
            if arguments.iter().any(SemScalarExpr::requires_argument_type) {
                return Ok(Self::deferred(name, arguments));
            }
            if let Some(overload) = overloads.resolve_from_catalog(context, &arg_types) {
                let args = Self::cast_operand_expression(
                    context,
                    name,
                    &arguments.to_vec().into(),
                    &overload,
                )?;
                return Ok(SemScalarExpr::function_call(&Self::new(
                    name,
                    &Some(overload),
                    &args,
                )));
            }
            context.report_analysis_problem(
                AnalysisProblem::function_overload_resolution_failed(name, &arg_types),
            )?;
        } else {
            context.report_analysis_problem(AnalysisProblem::function_not_found(name, span))?;
        }
        let mut args = Vec::new();
        for argument in arguments {
            args.push(FuncCallArgs::new(argument, &None));
        }
        Ok(SemScalarExpr::function_call(&Self::new(
            name,
            &None,
            &args.into(),
        )))
    }
}

impl SemScalarExprNode for FuncCallExpr {
    fn get_type(&self) -> Option<TypeReference> {
        let Some(overload) = &self.overload() else {
            return None;
        };
        overload.scaler_ret_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if let Some(overload) = &self.overload() {
            if overload.is_strict() {
                if overload.has_omitted_inputs(self.args().len(), self.modifiers().variadic()) {
                    return Some(false);
                }
                for arg in self.args() {
                    let arg_is_not_null = matches!(arg.arg_expr().is_not_null(), Some(true));

                    if !arg_is_not_null {
                        return Some(false);
                    }
                }
            }
            Some(overload.returns_not_null())
        } else {
            // when overload not resolved, returns None meaning Unknown
            None
        }
    }
}

#[cfg(test)]
mod test_func_call_expr_is_not_null {
    use crate::{
        names::FullName,
        sem::{
            FunctionParam, OverloadVariant, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
            scalar_expr::{
                SemScalarExprNode,
                func_call::{FuncCallArgs, FuncCallExpr},
            },
        },
    };

    use ::sql_fun_core::IVec;

    #[test]
    fn empty_expanded_variadic_is_not_an_omitted_default() {
        let int = PgBuiltInType::int4();
        let parameter =
            FunctionParam::new_variadig_param(&Some("xs".to_owned()), &Some(int.array_type()));
        let overload = OverloadVariant::new(&Some(int), &vec![parameter], true, true);
        let call = FuncCallExpr::new(
            &FullName::with_schema("public", "f"),
            &Some(overload),
            &IVec::default(),
        );
        assert_eq!(call.is_not_null(), Some(true));
    }

    #[test]
    fn test_is_not_null_strict_with_nullable_arg_returns_false() {
        let overload = OverloadVariant::new(&Some(PgBuiltInType::int4()), &vec![], true, true);
        let func_name = FullName::with_schema("public", "test_func");
        let arg_expr = ScalarConstExpr::null();
        let args: IVec<FuncCallArgs> = vec![FuncCallArgs::new(&arg_expr, &None)].into();
        let func_call = FuncCallExpr::new(&func_name, &Some(overload), &args);

        assert_eq!(Some(false), func_call.is_not_null());
    }

    #[test]
    fn test_is_not_null_non_strict_returns_returns_not_null() {
        let overload = OverloadVariant::new(&Some(PgBuiltInType::int4()), &vec![], false, true);
        let func_name = FullName::with_schema("public", "test_func");
        let arg_expr = ScalarConstExpr::null();
        let args: IVec<FuncCallArgs> = vec![FuncCallArgs::new(&arg_expr, &None)].into();
        let func_call = FuncCallExpr::new(&func_name, &Some(overload), &args);

        assert_eq!(Some(true), func_call.is_not_null());
    }

    #[test]
    fn test_is_not_null_strict_with_not_null_args_returns_returns_not_null() {
        let overload = OverloadVariant::new(&Some(PgBuiltInType::int4()), &vec![], true, false);
        let func_name = FullName::with_schema("public", "test_func");
        let arg_expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
        let args: IVec<FuncCallArgs> = vec![FuncCallArgs::new(&arg_expr, &None)].into();
        let func_call = FuncCallExpr::new(&func_name, &Some(overload), &args);

        assert_eq!(Some(false), func_call.is_not_null());
    }
}

impl FuncCallExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if self.signature_is_deferred() {
            return Ok(None);
        }
        let (name, arguments) = self.inference_call();
        if !arguments
            .iter()
            .any(|argument| argument.constraint_node().is_some())
        {
            return Ok(None);
        }
        if let Some(overloads) = self.candidates() {
            let types = ArgumentBindingCollection::from_expr_list(&arguments)
                .with_explicit_variadic(self.modifiers().variadic());
            for position in overloads.explicit_array_positions(&types) {
                parameters.array_access(context, &arguments[position], &[], false);
            }
            let signatures = overloads.call_signatures(context, &types);
            let mut result =
                parameters.call(context, &name.to_string(), &arguments, &signatures)?;
            result.retain_semantic_call(self);
            Ok(Some(result))
        } else {
            let mut result = parameters.operation(
                &name.to_string(),
                &arguments,
                &InputTypeConstraint::any(),
                None,
            );
            result.retain_semantic_call(self);
            Ok(Some(result))
        }
    }
}

#[cfg(test)]
mod test_function_argument_constraints {
    use crate::{
        context::ParseContext,
        names::AliasName,
        sem::TypeReference,
        sem::{AnalysisError, PgBuiltInType, SelectStatement, SemScalarExpr},
        test_helpers::{call_arguments::resolve, query::expression},
    };

    #[test]
    fn inference_preserves_reordered_positions_and_formal_associations() {
        let (mut query, context) = resolve(
            "SELECT f(c => $7::integer, a => $2::integer, b => $7::integer)",
            true,
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        for position in [7, 2] {
            assert_eq!(
                context
                    .parameter_constraints()
                    .parameter(position)
                    .unwrap()
                    .constraint()
                    .admissible_types(),
                Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
            );
        }
        let SemScalarExpr::Constrained(constrained) = expression(&query) else {
            panic!("constrained call")
        };
        let binding = constrained.function_binding().unwrap();
        assert!(binding.overload().is_some());
        assert_eq!(
            binding
                .argument_definition(0)
                .unwrap()
                .name()
                .map(String::as_str),
            Some("c")
        );
        struct Parameters(Vec<u32>);
        impl crate::sem::traversal::SemanticVisitorMut for Parameters {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Param(value) = expression {
                    self.0.push(value.position());
                }
                Ok(())
            }
        }
        let mut parameters = Parameters(Vec::new());
        query.walk(&mut parameters).unwrap();
        assert_eq!(parameters.0, [7, 2, 7]);
        let serialized = ::serde_json::to_value(&query).unwrap();
        let restored: SelectStatement = ::serde_json::from_value(serialized.clone()).unwrap();
        assert_eq!(::serde_json::to_value(restored).unwrap(), serialized);
    }

    #[rstest::rstest]
    #[case::scalar("SELECT f(a => $1) FROM f(a => 1) AS t")]
    #[case::table("SELECT t.* FROM f(a => $1::integer) AS t")]
    #[case::rows_from("SELECT t.* FROM ROWS FROM(f(a => $1)) AS t")]
    fn table_consumers_receive_call_constraints(#[case] sql: &str) {
        let (query, context) = resolve(sql, true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .admissible_types(),
            Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        let source = query
            .query_clauses()
            .unwrap()
            .body()
            .source_scope()
            .unwrap()
            .resolve_alias(&AliasName::from("t"))
            .unwrap()
            .0;
        if let Some(function) = source.deferred_function() {
            assert!(function.call().overload().is_some());
        }
    }

    #[rstest::rstest]
    #[case::scalar("SELECT poly(value => $1), f(a => $1)")]
    #[case::table("SELECT t.* FROM poly(value => $1) AS t, f(a => $1) AS other")]
    fn deferred_polymorphism_uses_other_occurrences(#[case] sql: &str) {
        let (query, context) = resolve(sql, true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(expression(&query).get_type(), None);
        assert_eq!(
            context
                .parameter_constraints()
                .result_column(0)
                .unwrap()
                .admissible_types(),
            Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
        );
    }

    #[test]
    fn named_polymorphism_retains_nondefault_assignments_and_correlations() {
        let (query, context) = resolve("SELECT same(b => $7, a => $2)", true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(expression(&query).get_type().is_none());
        let constraints = context.parameter_constraints();
        let bytea = TypeReference::from_master_type_name("bytea");
        assert!(constraints.allows(&std::collections::BTreeMap::from([
            (7, bytea.clone()),
            (2, bytea.clone())
        ])));
        assert!(!constraints.allows(&std::collections::BTreeMap::from([
            (7, bytea),
            (2, PgBuiltInType::int4())
        ])));
    }

    #[test]
    fn deferred_polymorphism_uses_a_nondefault_type_from_a_later_occurrence() {
        let (query, context) = resolve("SELECT poly(value=>$1), bytes(value=>$1)", true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            expression(&query).get_type(),
            Some(TypeReference::from_master_type_name("bytea"))
        );
    }

    #[test]
    fn deferred_variadic_array_keeps_element_and_array_identity_distinct() {
        let (query, context) = resolve("SELECT polyarray(VARIADIC xs=>$1), arr(xs=>$1)", true);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
    }
}

#[cfg(test)]
mod test_retains_function_modifiers_after_inference_without_duplicate_operands {
    use crate::{
        ast::Dialect,
        sem::{AnalysisError, PgBuiltInType, QueryNameResolution, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[test]
    fn retains_function_modifiers_after_inference_without_duplicate_operands() {
        let (mut query, context) = resolve(
            "SELECT sum($1 + 1 ORDER BY $2 + 1) FILTER (WHERE $3) OVER w WINDOW w AS ()",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        struct Calls {
            functions: usize,
            parameters: Vec<u32>,
        }
        impl crate::sem::traversal::SemanticVisitorMut for Calls {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Param(parameter) = expression {
                    self.parameters.push(parameter.position());
                }
                if let SemScalarExpr::Constrained(value) = expression
                    && let Some(binding) = value.function_binding()
                {
                    self.functions += 1;
                    assert_matches::assert_matches!(
                        binding.resolution(),
                        QueryNameResolution::Function(_)
                    );
                    assert_matches::assert_matches!(
                        binding
                            .modifiers()
                            .over()
                            .unwrap()
                            .reference()
                            .unwrap()
                            .resolution(),
                        QueryNameResolution::WindowAlias(0)
                    );
                    assert_eq!(
                        binding.modifiers().filter().unwrap().get_type(),
                        Some(PgBuiltInType::bool())
                    );
                }
                Ok(())
            }
        }
        let mut calls = Calls {
            functions: 0,
            parameters: Vec::new(),
        };
        query.walk(&mut calls).unwrap();
        calls.parameters.sort();
        assert_eq!(calls.parameters, [1, 2, 3]);
        assert_eq!(calls.functions, 1);
    }

    #[test]
    fn failed_function_name_lookup_survives_inference() {
        let (mut query, context) = resolve("SELECT absent($1)", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        struct Bindings(usize);
        impl crate::sem::traversal::SemanticVisitorMut for Bindings {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Constrained(value) = expression
                    && let Some(binding) = value.function_binding()
                {
                    assert_matches::assert_matches!(
                        binding.resolution(),
                        QueryNameResolution::NotFound
                    );
                    self.0 += 1;
                }
                Ok(())
            }
        }
        let mut bindings = Bindings(0);
        query.walk(&mut bindings).unwrap();
        assert_eq!(bindings.0, 1);
    }
}

impl FuncCallExpr {
    fn deferred(name: &FullName, arguments: &[SemScalarExpr]) -> SemScalarExpr {
        let args = arguments
            .iter()
            .map(|argument| FuncCallArgs::new(argument, &None))
            .collect::<Vec<_>>();
        SemScalarExpr::function_call(&Self::new(name, &None, &args.into()))
    }
}

impl FuncCallExpr {
    pub(crate) fn invalid_from(&self, context: &impl ContextParseContext, allow_set: bool) -> bool {
        (!allow_set && self.returns_set(context) == Some(true))
            || self.modifiers().over().is_some()
            || self
                .name_resolution()
                .function_identity()
                .is_some_and(|name| {
                    context.is_aggregate_impl(name)
                        || self.overload().as_ref().is_some_and(|overload| {
                            !context.function_returns_set(name, overload)
                                && !context.function_preserves_view_rows(name, overload)
                        })
                })
    }
}

impl FuncCallExpr {
    pub(crate) fn placement_result_type(&self) -> Option<TypeReference> {
        if let Some(result) = self.get_type() {
            Some(result)
        } else {
            self.candidates()?.common_scalar_result()
        }
    }
}
