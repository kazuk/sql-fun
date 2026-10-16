use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, FullName, OverloadVariant, SemScalarExpr, TypeReference,
    type_system::ArgumentBindingCollection,
};

use ::sql_fun_core::IVec;

mod implementation {
    use crate::sem::{FullName, FunctionParam, OverloadVariant, SemScalarExpr};

    use ::sql_fun_core::IVec;

    /// function call expression
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallExpr {
        func_name: FullName,
        overload: Option<OverloadVariant>,
        args: IVec<FuncCallArgs>,
    }

    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallArgs {
        arg_expr: SemScalarExpr,
        arg_definition: Option<FunctionParam>,
    }

    impl FuncCallExpr {
        /// create a instance
        #[must_use]
        pub fn new(
            func_name: &FullName,
            overload: &Option<OverloadVariant>,
            args: &IVec<FuncCallArgs>,
        ) -> Self {
            Self {
                func_name: func_name.clone(),
                overload: overload.clone(),
                args: args.clone(),
            }
        }
    }

    impl FuncCallExpr {
        /// resolved calling overload
        #[must_use]
        pub const fn overload(&self) -> &Option<OverloadVariant> {
            &self.overload
        }
    }

    impl FuncCallExpr {
        /// argument expression and definitions
        #[must_use]
        pub const fn args(&self) -> &IVec<FuncCallArgs> {
            &self.args
        }
    }

    impl FuncCallArgs {
        pub fn new(arg_expr: &SemScalarExpr, arg_definition: &Option<FunctionParam>) -> Self {
            Self {
                arg_expr: arg_expr.clone(),
                arg_definition: arg_definition.clone(),
            }
        }
    }

    impl FuncCallArgs {
        pub const fn arg_expr(&self) -> &SemScalarExpr {
            &self.arg_expr
        }
    }

    impl FuncCallArgs {
        pub const fn arg_definition(&self) -> &Option<FunctionParam> {
            &self.arg_definition
        }
    }
    impl FuncCallExpr {
        pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
            &mut self,
            visitor: &mut V,
        ) -> Result<(), crate::sem::AnalysisError> {
            let mut args = self.args.to_vec();
            for arg in &mut args {
                arg.arg_expr.walk(visitor)?;
            }
            self.args = args.into();
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

pub use self::implementation::{FuncCallArgs, FuncCallExpr};

impl FuncCallExpr {
    fn cast_operand_expression<TParseContext>(
        context: &mut TParseContext,
        func_name: &FullName,
        arg_exprs: &IVec<SemScalarExpr>,
        overload: &OverloadVariant,
    ) -> Result<IVec<FuncCallArgs>, AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let mut args = Vec::new();
        for (index, arg) in arg_exprs.iter().enumerate() {
            let mut arg = arg.clone();
            let arg_def = overload.get_arg_def_at(index);
            if let Some(arg_definition) = arg_def
                && let Some(arg_def_type) = arg_definition.get_type()
                && let Some(arg_val_type) = arg.get_type()
            {
                let Some(t) = context.get_type(arg_def_type.full_name()) else {
                    AnalysisError::raise_unexpected_input(&format!(
                        "type {arg_def_type} not found in context"
                    ))?
                };
                let Some(vt) = context.get_type(arg_val_type.full_name()) else {
                    AnalysisError::raise_unexpected_input(&format!(
                        "type {arg_val_type} not found in context"
                    ))?
                };

                if let Some(source_type) = vt.type_reference()
                    && let Some(target_type) = t.type_reference()
                    && let Some(cast) = context.get_implicit_cast(source_type, target_type)
                {
                    if !cast.is_no_conversion() {
                        arg = SemScalarExpr::wrap_implicit_cast(target_type, &arg);
                    }
                } else {
                    context.report_problem(
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

impl FuncCallExpr {
    pub(crate) fn resolve_common<TContext: crate::sem::resolution::ResolutionContext>(
        context: &mut TContext,
        name: &FullName,
        arguments: &[SemScalarExpr],
        span: &crate::StringSpan,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arg_types = ArgumentBindingCollection::from_expr_list(arguments);
        if let Some(overloads) = context.get_function_by_name(name) {
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
            context.report_problem(AnalysisProblem::function_overload_resolution_failed(
                name, &arg_types,
            ))?;
        } else {
            context.report_problem(AnalysisProblem::function_not_found(name, span))?;
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
mod test_func_call_expr_analyze_scalar_expr {
    use crate::sem::ScalarAnalysisContext;
    use crate::{
        sem::{
            FromClause, FunctionParam, OverloadVariant, PgBuiltInType, SchemaName, SemScalarExpr,
            WithClause,
        },
        syn::{
            KeywordKindOpt, Node, NodeInner, NodeList, ScanToken, ScanTokenBuilder, Token, TokenOpt,
        },
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use ::sql_fun_core::IVec;
    use ::testresult::TestResult;

    fn func_name_tokens() -> IVec<ScanToken> {
        let token = ScanTokenBuilder::default()
            .start(1)
            .end(1)
            .token(TokenOpt::from(Token::Ident))
            .keyword_kind(KeywordKindOpt::none())
            .build()
            .unwrap();
        vec![token.into()].into()
    }

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
mod test_func_call_expr_cast_operand_expression {
    use crate::{
        sem::{
            CastContext, CastDefinition, FullName, FunctionParam, OverloadVariant, PgBuiltInType,
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

        let args = super::FuncCallExpr::cast_operand_expression(
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

        let args = super::FuncCallExpr::cast_operand_expression(
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

#[cfg(test)]
mod test_func_call_expr_is_not_null {
    use crate::sem::scalar_expr::SemScalarExprNode;
    use crate::sem::scalar_expr::func_call::{FuncCallArgs, FuncCallExpr};
    use crate::sem::{FullName, OverloadVariant, PgBuiltInType, ScalarConstExpr, SemScalarExpr};

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
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut crate::sem::Resolution<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let (name, arguments) = self.inference_call();
        if !arguments
            .iter()
            .any(|argument| argument.constraint_node().is_some())
        {
            return Ok(None);
        }
        if let Some(overloads) = crate::sem::Catalog::get_function_by_name(context, name) {
            let signatures = overloads.parameter_signatures(context, &arguments);
            Ok(Some(parameters.call(
                context,
                &name.to_string(),
                &arguments,
                &signatures,
            )?))
        } else {
            Ok(Some(parameters.operation(
                &name.to_string(),
                &arguments,
                &crate::sem::InputTypeConstraint::any(),
                None,
            )))
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
