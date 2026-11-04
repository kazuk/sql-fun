mod implementation {
    use crate::{
        StringSpan,
        context::BaseParseContext,
        names::TableName,
        sem::{
            AnalysisError, FunctionModifiers, FunctionOverloadCollection, NamedArgExpr,
            QueryNameResolution,
            traversal::{SemanticVisitor, SemanticVisitorMut},
        },
    };

    use crate::{
        names::FullName,
        sem::{FunctionParam, OverloadVariant, SemScalarExpr},
    };

    use ::sql_fun_core::IVec;

    /// function call expression
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallExpr {
        func_name: FullName,
        name_resolution: QueryNameResolution,
        #[serde(default)]
        candidates: Option<Vec<OverloadVariant>>,
        overload: Option<OverloadVariant>,
        #[serde(default)]
        signature_deferred: bool,
        #[serde(deserialize_with = "deserialize_arguments")]
        args: IVec<FuncCallArgs>,
        #[serde(default)]
        modifiers: Box<FunctionModifiers>,
    }

    fn deserialize_arguments<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<IVec<FuncCallArgs>, D::Error> {
        let arguments = <Vec<FuncCallArgs> as serde::Deserialize>::deserialize(deserializer)?;
        Ok(arguments
            .into_iter()
            .enumerate()
            .map(|(position, argument)| argument.at_position(position))
            .collect::<Vec<_>>()
            .into())
    }

    /// A supplied argument, its call position, and any independently established formal association.
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallArgs {
        arg_expr: SemScalarExpr,
        arg_definition: Option<FunctionParam>,
        #[serde(default)]
        position: usize,
    }

    /// Function identity and SQL modifiers for a parameter-dependent call.
    /// Operands are owned by the enclosing constrained expression.
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FunctionBinding {
        name: FullName,
        resolution: QueryNameResolution,
        modifiers: Box<FunctionModifiers>,
    }
    impl FunctionBinding {
        /// Resolved function identity (or original name after a failed lookup).
        pub const fn name(&self) -> &FullName {
            &self.name
        }
        /// Completed name lookup, independent of overload selection.
        pub const fn resolution(&self) -> &QueryNameResolution {
            &self.resolution
        }
        /// SQL modifiers owned by this call.
        pub fn modifiers(&self) -> &FunctionModifiers {
            &self.modifiers
        }
        pub(crate) fn walk<V: SemanticVisitorMut>(
            &mut self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            self.modifiers.walk(visitor)
        }
        pub(crate) fn visit<V: SemanticVisitor>(
            &self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            self.modifiers.visit(visitor)
        }
    }
    impl FuncCallExpr {
        pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
            self.name_resolution.rename_relation(old, new);
            if let Some(overload) = &mut self.overload {
                overload.rename_relation(old, new);
            }
            if let Some(candidates) = &mut self.candidates {
                for candidate in candidates {
                    candidate.rename_relation(old, new);
                }
            }
            let mut arguments = self.args.to_vec();
            for argument in &mut arguments {
                if let Some(definition) = &mut argument.arg_definition {
                    definition.rename_relation(old, new);
                }
            }
            self.args = arguments.into();
        }
        pub(crate) fn canonicalize_grouping_key(&mut self) {
            if let Some(name) = self.name_resolution.function_identity() {
                self.func_name = name.clone();
            }
            self.candidates = None;
        }

        pub(crate) fn binding_snapshot(&self) -> FunctionBinding {
            FunctionBinding {
                name: self.func_name.clone(),
                resolution: self.name_resolution.clone(),
                modifiers: self.modifiers.clone(),
            }
        }
    }
    impl FuncCallExpr {
        /// create a instance
        #[must_use]
        pub fn new(
            func_name: &FullName,
            overload: &Option<OverloadVariant>,
            args: &IVec<FuncCallArgs>,
        ) -> Self {
            let mut positioned = Vec::new();
            for argument in args {
                positioned.push(argument.at_position(positioned.len()));
            }
            Self {
                func_name: func_name.clone(),
                candidates: None,
                name_resolution: if overload.is_some() {
                    QueryNameResolution::function(func_name)
                } else {
                    QueryNameResolution::not_found()
                },
                overload: overload.clone(),
                signature_deferred: false,
                args: positioned.into(),
                modifiers: Box::default(),
            }
        }
    }

    #[cfg(test)]
    mod test_supplied_arguments {
        use crate::{
            names::FullName,
            sem::{
                FunctionParam, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
                scalar_expr::func_call::implementation::{FuncCallArgs, FuncCallExpr},
            },
        };

        #[test]
        fn owning_call_assigns_positions_without_losing_known_associations() {
            let value = SemScalarExpr::literal(&ScalarConstExpr::new_integer(1));
            let formal = FunctionParam::new_input_param(
                &Some("known".to_owned()),
                &Some(PgBuiltInType::int4()),
                &None,
            );
            let named = SemScalarExpr::named_argument("known", &value, &None);
            let arguments = vec![
                FuncCallArgs::new(&value, &None),
                FuncCallArgs::new(&named, &Some(formal.clone())),
            ]
            .into();
            let call = FuncCallExpr::new(&FullName::from_local_name("f"), &None, &arguments);
            assert_eq!(call.args()[0].position(), 0);
            assert_eq!(call.args()[0].supplied_name(), None);
            assert_eq!(call.args()[1].position(), 1);
            assert_eq!(call.args()[1].supplied_name(), Some("known"));
            assert_eq!(call.args()[1].arg_definition(), &Some(formal));
            assert_eq!(call.args()[1].value(), &value);
            let restored: FuncCallExpr =
                ::serde_json::from_str(&::serde_json::to_string(&call).unwrap()).unwrap();
            assert_eq!(restored, call);
            let mut legacy = ::serde_json::to_value(&call).unwrap();
            for argument in legacy["args"].as_array_mut().unwrap() {
                argument.as_object_mut().unwrap().remove("position");
            }
            let restored: FuncCallExpr = ::serde_json::from_value(legacy).unwrap();
            assert_eq!(restored, call);
        }
    }

    impl FuncCallExpr {
        pub(super) const fn defer_signature(&mut self) {
            self.signature_deferred = true;
        }
        /// Whether signature binding is explicitly deferred, independently of name lookup.
        pub const fn signature_is_deferred(&self) -> bool {
            self.signature_deferred
        }
        pub(crate) fn set_candidates(&mut self, candidates: Option<Vec<OverloadVariant>>) {
            self.candidates = candidates;
        }
        pub(crate) fn returns_set(&self, context: &impl BaseParseContext) -> Option<bool> {
            let name = self.name_resolution().function_identity()?;
            if let Some(overload) = self.overload() {
                Some(context.function_returns_set(name, overload))
            } else {
                self.candidates()?.returns_set(context, name)
            }
        }
        pub(crate) fn candidates(&self) -> Option<FunctionOverloadCollection> {
            self.candidates
                .as_deref()
                .map(FunctionOverloadCollection::new)
        }
        pub(crate) fn set_name_resolution(&mut self, outcome: QueryNameResolution) {
            self.name_resolution = outcome;
        }
        /// Completed function-name lookup, independent of overload selection.
        pub const fn name_resolution(&self) -> &QueryNameResolution {
            &self.name_resolution
        }
        pub(crate) fn set_modifiers(&mut self, modifiers: FunctionModifiers) {
            *self.modifiers = modifiers;
        }
        /// SQL modifiers retained alongside the resolved function binding.
        pub fn modifiers(&self) -> &FunctionModifiers {
            &self.modifiers
        }
    }
    impl FuncCallExpr {
        pub(crate) const fn name(&self) -> &FullName {
            &self.func_name
        }

        /// Selected overload, independently of name lookup.
        #[must_use]
        pub const fn overload(&self) -> &Option<OverloadVariant> {
            &self.overload
        }
    }

    impl FuncCallExpr {
        pub(crate) fn argument_values(&self) -> IVec<SemScalarExpr> {
            self.args
                .iter()
                .map(|argument| argument.value().clone())
                .collect::<Vec<_>>()
                .into()
        }
        /// argument expression and definitions
        #[must_use]
        pub const fn args(&self) -> &IVec<FuncCallArgs> {
            &self.args
        }
    }

    impl FuncCallArgs {
        /// Preserve an expression and an already known formal association. The owning call assigns its position.
        pub fn new(arg_expr: &SemScalarExpr, arg_definition: &Option<FunctionParam>) -> Self {
            Self {
                arg_expr: arg_expr.clone(),
                arg_definition: arg_definition.clone(),
                position: 0,
            }
        }
    }

    impl FuncCallArgs {
        fn at_position(&self, position: usize) -> Self {
            let mut arg = self.clone();
            arg.position = position;
            arg
        }
        /// Zero-based position in the supplied call, independent of a formal index or `$n`.
        pub const fn position(&self) -> usize {
            self.position
        }
        /// Optional supplied name. `None` identifies a positional argument.
        pub fn supplied_name(&self) -> Option<&str> {
            self.arg_expr.as_named_argument().map(NamedArgExpr::name)
        }
        /// The value inside a named wrapper, or the positional expression itself.
        pub fn value(&self) -> &SemScalarExpr {
            self.arg_expr
                .as_named_argument()
                .map_or(&self.arg_expr, NamedArgExpr::value)
        }
        /// Source span of the supplied argument name, when reliably available.
        pub const fn source_span(&self) -> Option<&StringSpan> {
            if let Some(named) = self.arg_expr.as_named_argument() {
                named.source_span()
            } else {
                None
            }
        }
        /// Supplied expression, including its named wrapper when present.
        pub const fn arg_expr(&self) -> &SemScalarExpr {
            &self.arg_expr
        }
    }

    impl FuncCallArgs {
        /// Already established formal association; `None` does not imply an invalid argument.
        pub const fn arg_definition(&self) -> &Option<FunctionParam> {
            &self.arg_definition
        }
    }
    impl FuncCallExpr {
        pub(crate) fn walk<V: SemanticVisitorMut>(
            &mut self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            let mut args = self.args.to_vec();
            for arg in &mut args {
                arg.arg_expr.walk(visitor)?;
            }
            self.args = args.into();
            self.modifiers.walk(visitor)?;
            Ok(())
        }
        pub(crate) fn visit<V: SemanticVisitor>(
            &self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            for arg in self.args.iter() {
                arg.arg_expr.visit(visitor)?;
            }
            self.modifiers.visit(visitor)?;
            Ok(())
        }
        pub(crate) fn inference_call(&self) -> (&FullName, Vec<SemScalarExpr>) {
            (
                &self.func_name,
                self.args.iter().map(|arg| arg.arg_expr.clone()).collect(),
            )
        }
    }
}

use crate::{
    StringSpan,
    context::ParseContext as ContextParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, FunctionOverloadCollection, InputTypeConstraint,
        OverloadVariant, ParseContext, QueryNameResolution, SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution, problem::CallArgumentProblem,
        scalar_expr::SemScalarExprNode, type_system::ArgumentBindingCollection,
    },
};

use ::sql_fun_core::IVec;

pub use self::implementation::{FuncCallArgs, FuncCallExpr, FunctionBinding};

impl FuncCallExpr {
    pub(crate) fn resolve_supplied(
        context: &mut impl ParseContext,
        name: &FullName,
        arguments: &[SemScalarExpr],
        argument_spans: &[Option<StringSpan>],
        span: &Option<StringSpan>,
    ) -> Result<Self, AnalysisError> {
        let mut names = std::collections::HashSet::new();
        let mut args = Vec::new();
        for argument in arguments {
            let position = args.len();
            if let Some(named) = argument.as_named_argument() {
                if !names.insert(named.name()) {
                    context.report_analysis_problem(AnalysisProblem::call_argument(
                        CallArgumentProblem::duplicate_name(
                            named.name(),
                            &named.source_span().map(Into::into),
                        ),
                    ))?;
                }
            } else if !names.is_empty() {
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
            args.push(FuncCallArgs::new(argument, &None));
        }
        let candidates = context.get_function_by_name(name);
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
        call.set_candidates(candidates.map(FunctionOverloadCollection::into_snapshot));
        call.defer_signature();
        Ok(call)
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
        let mut args = Vec::new();
        for (index, arg) in arg_exprs.iter().enumerate() {
            let mut arg = arg.clone();
            let arg_def = overload.call_argument(index);
            if let Some(arg_definition) = arg_def
                && !arg_definition.accepts_any_type()
                && let Some(arg_def_type) = arg_definition.call_argument_type()
                && let Some(arg_val_type) = arg.get_type()
            {
                let Some(t) = arg_def_type.definition_from_catalog(context) else {
                    AnalysisError::raise_unexpected_input(&format!(
                        "type {arg_def_type} not found in context"
                    ))?
                };
                if let Some(cast) = arg_val_type.implicit_match_context(&arg_def_type, context) {
                    if !cast.is_no_conversion() {
                        arg = SemScalarExpr::wrap_implicit_cast(&arg_def_type, &arg);
                    }
                } else {
                    context.report_analysis_problem(
                        AnalysisProblem::function_arg_implicit_cast_not_found(
                            func_name, index, t, &arg,
                        ),
                    )?;
                }
            }

            args.push(FuncCallArgs::new(&arg, &arg_def.cloned()));
        }
        Ok(args.into())
    }
}

#[cfg(test)]
mod test_func_call_expr_cast_operand_expression {
    use crate::sem::scalar_expr::func_call::FuncCallExpr;

    use crate::{
        names::FullName,
        sem::{
            CastContext, CastDefinition, FunctionParam, OverloadVariant, PgBuiltInType,
            ScalarConstExpr, SemScalarExpr,
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
            OverloadVariant, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
            scalar_expr::{
                SemScalarExprNode,
                func_call::{FuncCallArgs, FuncCallExpr},
            },
        },
    };

    use ::sql_fun_core::IVec;

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
            let signatures = overloads.parameter_signatures(context, &arguments);
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
