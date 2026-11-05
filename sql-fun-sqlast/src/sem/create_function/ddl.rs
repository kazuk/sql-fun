//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::ddl::{Function as AstFunction, FunctionOption as AstFunctionOption},
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnCollection, ColumnDefinition, CreateFunction,
        FunctionOverloadCollection, FunctionParam, OverloadVariant, SemAst, TypeReference,
    },
};

impl CreateFunction {
    pub(crate) fn from_ast(
        ast: &AstFunction,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_function_null_input_semantics {
    use crate::{
        ast::{TryFromWithTokens, ddl::Function},
        sem::CreateFunction,
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext},
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::called_on_null("CALLED ON NULL INPUT", false)]
    #[case::returns_null("RETURNS NULL ON NULL INPUT", true)]
    #[case::strict("STRICT", true)]
    fn honors_the_explicit_null_input_choice(#[case] option: &str, #[case] expected: bool) {
        let syntax = SynBuilder::new().parse(&format!(
            "CREATE FUNCTION f() RETURNS integer LANGUAGE sql {option} AS 'SELECT 1'"
        ));
        let function = Function::try_from_with_tokens(
            &syntax.as_create_function_stmt().as_inner().unwrap(),
            &IVec::default(),
        )
        .unwrap();
        let mut context = TestParseContext::default();
        let resolved = CreateFunction::from_ast(&function, &mut context).unwrap();
        assert_eq!(
            expected,
            resolved.overloads().get_at(0).unwrap().is_strict()
        );
    }
}

impl CreateFunction {
    pub(crate) fn from_ast_inner(
        ast: &AstFunction,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(ast.name())?;
        if ast.procedure() || ast.result_type().is_none() {
            let mut params = Vec::new();
            let mut outputs = Vec::new();
            for param in ast.parameters() {
                let resolved = FunctionParam::from_ast(param, context)?;
                if param.mode().is_output() {
                    outputs.push(resolved.get_type().cloned());
                }
                params.push(resolved);
            }
            let ret_type = if ast.procedure() {
                None
            } else if outputs.len() == 1 {
                outputs.pop().flatten()
            } else if !outputs.is_empty() {
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "record"),
                    false,
                ))
            } else {
                return AnalysisError::raise_unexpected_none("create_function.return_type");
            };
            let strict = ast.options().iter().any(AstFunctionOption::is_strict);
            let overload = OverloadVariant::new(&ret_type, &params, strict, false);
            return Ok(
                Self::new(&name, &FunctionOverloadCollection::new(&[overload]), &None)
                    .with_procedure_kind(ast.procedure()),
            );
        }
        let Some(result_type) = &ast.result_type() else {
            return AnalysisError::raise_unexpected_none("create_function.return_type");
        };
        let ret_name = FullName::from_ast_name(result_type.name())?;
        let returning = if result_type.set_of() {
            if ret_name.name().as_str() == "record" {
                let mut columns = Vec::new();
                for param in ast.parameters() {
                    if param.mode().is_output() {
                        let ty = TypeReference::from_full_name(FullName::from_ast_name(
                            param.type_name().name(),
                        )?);
                        columns.push(ColumnDefinition::new(
                            &Some(ColumnName::from(param.name().as_str())),
                            Some(&ty),
                            None,
                            &[],
                        ));
                    }
                }
                Some(ColumnCollection::new(columns))
            } else if let Some(ty) = context.get_type(&ret_name) {
                if ty.is_composite_type() {
                    Some(ty.get_composite_type_columns())
                } else {
                    ty.type_reference().map(|ty| {
                        ColumnCollection::new(vec![ColumnDefinition::new(
                            &None,
                            Some(ty),
                            None,
                            &[],
                        )])
                    })
                }
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::function_result_set_type_not_found(&ret_name),
                )?;
                Some(ColumnCollection::new(Vec::new()))
            }
        } else {
            None
        };
        let mut params = Vec::new();
        for param in ast.parameters() {
            params.push(FunctionParam::from_ast(param, context)?);
        }
        let ret_type = if let Some(ty) = context.get_type(&ret_name) {
            ty.type_reference().cloned()
        } else {
            None
        };
        let strict = ast.options().iter().any(AstFunctionOption::is_strict);
        let overload = OverloadVariant::new(&ret_type, &params, strict, false);
        Ok(Self::new(
            &name,
            &FunctionOverloadCollection::new(&[overload]),
            &returning,
        ))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstFunction,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let function = Self::from_ast(ast, &mut context)?.with_replace(ast.replace());
        let preserves_rows = ast
            .result_type()
            .as_ref()
            .is_none_or(|result| !result.set_of())
            && !ast
                .options()
                .iter()
                .any(|option| matches!(option, AstFunctionOption::Window));
        let context = context
            .apply_create_function(&function)?
            .apply_function_view_rows(&function, preserves_rows)?;
        Ok(AstAndContextPair::new(
            SemAst::create_function(&function),
            context,
        ))
    }
}
