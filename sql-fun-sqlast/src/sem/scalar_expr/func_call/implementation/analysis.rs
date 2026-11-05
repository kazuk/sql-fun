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
