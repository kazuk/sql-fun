//! Analysis owned by the semantic definitions in this module.

use crate::sem::create_function::FunctionDeclarationOutcome;
use crate::{
    AstAndContextPair, StringSpan,
    ast::{
        Dialect, TypeName,
        ddl::{Function as AstFunction, FunctionOption as AstFunctionOption},
    },
    context::ParseContext,
    names::{ColumnName, FullName, SchemaName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnCollection, ColumnDefinition, CreateFunction,
        FunctionOverloadCollection, FunctionParam, OverloadVariant, SemAst, TypeDefinition,
        TypeReference, problem::CreationProblem,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShellPolicy {
    Create,
    Reuse,
    Reject,
}

impl ShellPolicy {
    fn for_function(ast: &AstFunction, dialect: Dialect) -> Self {
        if dialect != Dialect::postgres() {
            return Self::Reuse;
        }
        match ast.options().iter().find_map(|option| {
            if let AstFunctionOption::Language(language) = option {
                Some(language.as_str())
            } else {
                None
            }
        }) {
            Some("c" | "internal") => Self::Create,
            Some("sql") => Self::Reject,
            _ => Self::Reuse,
        }
    }
}

impl FunctionDeclarationOutcome {
    pub(super) fn reject(
        &mut self,
        name: &FullName,
        reason: String,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        *self = Self::Rejected(reason.clone());
        context.report_analysis_problem(AnalysisProblem::invalid_function_signature(name, reason))
    }
}

impl CreateFunction {
    pub(crate) fn from_ast(
        ast: &AstFunction,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::analyze_declaration(ast, context);
        context.finish_analysis(result)
    }

    fn analyze_declaration(
        ast: &AstFunction,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut outcome = FunctionDeclarationOutcome::default();
        let mut function =
            Self::from_ast_inner(ast, context, &mut outcome)?.with_replace(ast.replace());
        if outcome.shell().is_some() && context.analysis_state().has_errors() {
            outcome = FunctionDeclarationOutcome::Rejected("invalid routine signature".to_owned());
        }
        if matches!(outcome, FunctionDeclarationOutcome::Applicable(_))
            && let Some(conflict) = function.declaration_conflict(context)
        {
            outcome = FunctionDeclarationOutcome::Rejected(
                "routine signature conflicts with an existing declaration".to_owned(),
            );
            context.report_analysis_problem(conflict)?;
        }
        function.outcome = outcome;
        Ok(function)
    }

    pub(super) fn declaration_conflict(
        &self,
        context: &impl ParseContext,
    ) -> Option<AnalysisProblem> {
        let mut name = self.name().clone();
        if name.schema().is_none() {
            name = name.set_schema(context.get_default_schema()?);
        }
        let existing = context.routine_definitions(&name);
        let mut candidates = Vec::new();
        for candidate in self.split_definitions() {
            if let Some(previous) = candidate.previous_definition(&existing, &candidates)
                && (!self.replace() || !candidate.can_replace(previous))
            {
                let span = context
                    .get_source_path_with_span()
                    .map_or(StringSpan::default(), |source| *source.span());
                return Some(AnalysisProblem::creation(CreationProblem::routine(
                    &name,
                    &candidate.ownership_identity(&name),
                    &span,
                )));
            }
            candidates.push(candidate);
        }
        None
    }
}

impl CreateFunction {
    fn from_ast_inner(
        ast: &AstFunction,
        context: &mut impl ParseContext,
        declaration: &mut FunctionDeclarationOutcome,
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
        let shell_policy = ShellPolicy::for_function(ast, context.dialect());
        let mut params = Vec::new();
        for param in ast.parameters() {
            let resolved = FunctionParam::from_ast(param, context)?;
            if shell_policy == ShellPolicy::Create && resolved.get_type().is_none() {
                let missing = param.type_name().name().parts().join(".");
                declaration.reject(
                    &name,
                    format!("argument type {missing} does not exist"),
                    context,
                )?;
            }
            params.push(resolved);
        }
        let ret_type =
            Self::resolve_return_type(result_type, &name, shell_policy, declaration, context)?;
        if declaration.shell().is_some()
            && ast
                .parameters()
                .iter()
                .any(|param| param.mode().is_output())
        {
            declaration.reject(
                &name,
                "return type does not match the OUT parameters".to_owned(),
                context,
            )?;
        }
        let returning = Self::returning_columns(ast, result_type, ret_type.as_ref(), context)?;
        let strict = ast.options().iter().any(AstFunctionOption::is_strict);
        let overload = OverloadVariant::new(&ret_type, &params, strict, false);
        Ok(Self::new(
            &name,
            &FunctionOverloadCollection::new(&[overload]),
            &returning,
        ))
    }
}

impl CreateFunction {
    fn resolve_return_type(
        result: &TypeName,
        function: &FullName,
        shell_policy: ShellPolicy,
        declaration: &mut FunctionDeclarationOutcome,
        context: &mut impl ParseContext,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let array = !result.array_bounds().is_empty();
        if result.percent_type() {
            let reference = TypeReference::from_percent_type(result, context)?;
            if (shell_policy != ShellPolicy::Reuse || result.set_of()) && reference.is_none() {
                let name = result.name().parts().join(".");
                declaration.reject(
                    function,
                    format!("column type {name}%TYPE does not exist"),
                    context,
                )?;
            }
            return Ok(reference);
        }
        let name = FullName::from_ast_name(result.name())?;
        if let Some(definition) = context
            .get_declared_type(&name)
            .filter(|definition| !array || definition.has_postgres_array_type())
        {
            if definition.is_shell() {
                let reason = if shell_policy == ShellPolicy::Reject {
                    Some(format!("SQL function cannot return shell type {name}"))
                } else if !result.modifiers().is_empty() {
                    Some(format!(
                        "type modifier cannot be specified for shell type {name}"
                    ))
                } else {
                    None
                };
                if let Some(reason) = reason {
                    declaration.reject(function, reason, context)?;
                    return Ok(None);
                }
            }
            if array {
                return Ok(definition
                    .full_name()
                    .map(|name| TypeReference::concrete_type_ref(name, true)));
            }
            return Ok(definition.type_reference().cloned());
        }
        if matches!(declaration, FunctionDeclarationOutcome::Rejected(_)) {
            return Ok(None);
        }
        if shell_policy != ShellPolicy::Create {
            if shell_policy == ShellPolicy::Reject || result.set_of() {
                declaration.reject(
                    function,
                    format!("return type {name} does not exist"),
                    context,
                )?;
            }
            return Ok(None);
        }
        if !result.modifiers().is_empty() {
            declaration.reject(
                function,
                format!("type modifier cannot be specified for shell type {name}"),
                context,
            )?;
            return Ok(None);
        }
        let Some(name) = Self::shell_creation_name(&name, function, declaration, context)? else {
            return Ok(None);
        };
        if !context.permits_type_creation(&name)? {
            *declaration =
                FunctionDeclarationOutcome::Rejected(format!("type {name} already exists"));
            return Ok(None);
        }
        // PostgreSQL 15–18 bind a missing array return to the nominal shell,
        // with the SETOF flag independent of the array spelling.
        let reference = TypeReference::concrete_type_ref(&name, false);
        *declaration = FunctionDeclarationOutcome::Applicable(Some(Box::new(
            TypeDefinition::Shell(reference.clone()),
        )));
        Ok(Some(reference))
    }

    pub(super) fn shell_creation_name(
        name: &FullName,
        function: &FullName,
        declaration: &mut FunctionDeclarationOutcome,
        context: &mut impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let schema_exists = |schema: &SchemaName| {
            matches!(
                schema.as_str(),
                "public" | "pg_catalog" | "information_schema"
            ) || context.schema_definition(schema.as_str()).is_some()
        };
        if let Some(schema) = name.schema() {
            if schema_exists(schema) {
                return Ok(Some(name.clone()));
            }
            declaration.reject(function, format!("schema {schema} does not exist"), context)?;
        } else if let Some(schema) = context
            .get_search_path()
            .iter()
            .find(|schema| schema_exists(schema))
        {
            return Ok(Some(name.replace_schema(schema)));
        } else {
            declaration.reject(
                function,
                "no schema selected for the shell type".to_owned(),
                context,
            )?;
        }
        Ok(None)
    }

    fn returning_columns(
        ast: &AstFunction,
        result: &TypeName,
        reference: Option<&TypeReference>,
        context: &mut impl ParseContext,
    ) -> Result<Option<ColumnCollection>, AnalysisError> {
        if !result.set_of() {
            return Ok(None);
        }
        let Some(reference) = reference else {
            return Ok(Some(ColumnCollection::new(Vec::new())));
        };
        if reference
            == &TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "record"),
                false,
            )
        {
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
            return Ok(Some(ColumnCollection::new(columns)));
        }
        if let Some(name) = reference.full_name()
            && reference.is_array() != Some(true)
            && let Some(ty) = context.get_type(name)
            && ty.is_composite_type()
        {
            return Ok(Some(ty.get_composite_type_columns()));
        }
        Ok(Some(ColumnCollection::new(vec![ColumnDefinition::new(
            &None,
            Some(reference),
            None,
            &[],
        )])))
    }
}

#[cfg(test)]
mod test_honors_the_explicit_null_input_choice {
    use crate::{
        ast::{Dialect, StatementNode},
        parse_statements,
        sem::CreateFunction,
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::called_on_null("CALLED ON NULL INPUT", false)]
    #[case::returns_null("RETURNS NULL ON NULL INPUT", true)]
    #[case::strict("STRICT", true)]
    fn honors_the_explicit_null_input_choice(#[case] option: &str, #[case] expected: bool) {
        let sql =
            format!("CREATE FUNCTION f() RETURNS integer LANGUAGE sql {option} AS 'SELECT 1'");
        let syntax = parse_statements(&sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .into_test_node();
        let StatementNode::Function(function) = syntax else {
            panic!("CREATE FUNCTION syntax")
        };
        let mut context = TestParseContext::default();
        let resolved = CreateFunction::from_ast(&function, &mut context).unwrap();
        assert_eq!(
            expected,
            resolved.overloads().get_at(0).unwrap().is_strict()
        );
    }
}

impl CreateFunction {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstFunction,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut function = Self::from_ast(ast, &mut context)?;
        function.source = context.get_source_path_with_span().cloned();
        let preserves_rows = ast
            .result_type()
            .as_ref()
            .is_none_or(|result| !result.set_of())
            && !ast
                .options()
                .iter()
                .any(|option| matches!(option, AstFunctionOption::Window));
        let context = context.apply_create_function(&function, preserves_rows)?;
        Ok(AstAndContextPair::new(
            SemAst::create_function(&function),
            context,
        ))
    }
}

#[cfg(test)]
mod implicit_shell_tests {
    use crate::{
        ast::{ObjectKind, StatementNode},
        context::{
            BaseParseContext, ExtensionInfoRead, FunctionInfoRead, OwnershipInfoRead, ParseContext,
        },
        names::FullName,
        sem::{CreateFunction, OwnershipIdentity, SemAst, TypeReference},
        test_helpers::ownership::{apply_ownership_sql, ownership_context, ownership_statement},
    };

    #[rstest::rstest]
    #[case("shell_type", "C", false)]
    #[case("SETOF shell_type", "internal", true)]
    #[case("shell_type ARRAY", "internal", false)]
    #[case("SETOF public.shell_type[][]", "C", true)]
    fn publishes_shell_and_routine(
        #[case] result: &str,
        #[case] language: &str,
        #[case] returns_set: bool,
    ) -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE SCHEMA routines")?.into_context();
        let application = apply_ownership_sql(
            context,
            &format!(
                "CREATE FUNCTION routines.f() RETURNS {result} LANGUAGE {language} AS 'symbol'"
            ),
        )?;
        let context = application.context();
        let name = FullName::with_schema("public", "shell_type");
        let reference = TypeReference::concrete_type_ref(&name, false);
        assert_eq!(
            context.get_declared_type(&name).unwrap().type_reference(),
            Some(&reference),
        );
        assert!(context.get_type(&name).is_none());
        let function = FullName::with_schema("routines", "f");
        let definitions = context.routine_definitions(&function);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].catalog_returns_set(), returns_set);
        assert_eq!(
            definitions[0].overloads().get_at(0).unwrap().ret_type(),
            &Some(reference),
        );
        assert!(context.problems().is_empty());
        Ok(())
    }

    #[rstest::rstest]
    #[case("integer[]")]
    #[case("source_columns.item%TYPE")]
    fn existing_array_returns_keep_their_array_type(
        #[case] result: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE TABLE source_columns (item integer[])",
        )?
        .into_context();
        let application = apply_ownership_sql(
            context,
            &format!("CREATE FUNCTION f() RETURNS {result} LANGUAGE internal AS 'int4in'"),
        )?;
        let context = application.context();
        let function = FullName::with_schema("public", "f");
        let overloads = context.get_function_impl(&function).unwrap();
        let reference =
            TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", "int4"), true);
        assert_eq!(overloads.get_at(0).unwrap().ret_type(), &Some(reference));
        assert!(context.problems().is_empty());
        Ok(())
    }

    #[rstest::rstest]
    #[case("f(shell_type)", "shell_type")]
    #[case("f(integer)", "shell_type")]
    #[case("f()", "missing_schema.shell_type")]
    #[case("f()", "shell_type(4)")]
    #[case("f(OUT value integer)", "shell_type")]
    #[case("f()", "missing_table.missing_column%TYPE")]
    #[case("f()", "explicit_shell[]")]
    fn rejected_declaration_publishes_no_effects(
        #[case] signature: &str,
        #[case] result: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION f(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let context = apply_ownership_sql(context, "CREATE TYPE explicit_shell")?.into_context();
        let rejected = apply_ownership_sql(
            context,
            &format!("CREATE FUNCTION {signature} RETURNS {result} LANGUAGE internal AS 'int4in'"),
        )?;
        let SemAst::CreateFunction(statement) = rejected.ast() else {
            panic!("expected analyzed function declaration");
        };
        assert!(!statement.is_applicable());
        assert!(statement.rejection_reason().is_some());
        assert!(statement.implicit_shell().is_none());
        let context = rejected.context();
        let function = FullName::with_schema("public", "f");
        assert_eq!(context.routine_definitions(&function).len(), 1);
        for schema in ["public", "missing_schema"] {
            assert!(
                context
                    .get_declared_type(&FullName::with_schema(schema, "shell_type"))
                    .is_none()
            );
        }
        assert!(
            context
                .get_declared_type(&FullName::with_schema("public", "explicit_shell"))
                .is_some_and(crate::sem::TypeDefinition::is_shell)
        );
        assert!(!context.problems().is_empty());
        Ok(())
    }

    #[rstest::rstest]
    #[case("shell_type")]
    #[case("integer")]
    fn suppressed_diagnostics_cannot_publish_rejected_shells(
        #[case] argument: &str,
    ) -> testresult::TestResult {
        let mut context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION f(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        )?
        .into_context();
        let StatementNode::Function(ast) = ownership_statement(&format!(
            "CREATE FUNCTION f({argument}) RETURNS shell_type LANGUAGE internal AS 'int4in'"
        ))?
        else {
            panic!("expected function declaration");
        };
        context.begin_parameter_type_analysis();
        let function = CreateFunction::analyze_declaration(&ast, &mut context)?;
        assert!(!function.is_applicable());
        assert!(function.rejection_reason().is_some());
        assert!(function.implicit_shell().is_none());
        assert_eq!(context.analysis_state_mut().take_problems().len(), 1);
        let context = context.apply_create_function(&function, false)?;
        assert!(context.problems().is_empty());
        let function = FullName::with_schema("public", "f");
        let definitions = context.routine_definitions(&function);
        assert_eq!(definitions.len(), 1);
        let overload = definitions[0].overloads().get_at(0).unwrap();
        assert!(context.function_preserves_view_rows(&function, overload));
        assert!(
            context
                .get_declared_type(&FullName::with_schema("public", "shell_type"))
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn analysis_only_retains_rejected_sql_declaration() -> testresult::TestResult {
        let StatementNode::Function(ast) =
            ownership_statement("CREATE FUNCTION f() RETURNS missing LANGUAGE sql AS 'SELECT 1'")?
        else {
            panic!("expected function declaration");
        };
        let mut context = ownership_context();
        let function = CreateFunction::from_ast(&ast, &mut context)?;
        assert!(!function.is_applicable());
        assert_eq!(
            function.rejection_reason(),
            Some("return type missing does not exist")
        );
        assert!(function.implicit_shell().is_none());
        assert!(
            context
                .routine_definitions(&FullName::with_schema("public", "f"))
                .is_empty()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn completion_preserves_the_routine_return_binding(
        #[case] explicit: bool,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        context.enter_extension_script("shell_extension", Some("public"));
        if explicit {
            context = apply_ownership_sql(context, "CREATE TYPE public.shell_type")?.into_context();
        }
        for sql in [
            "CREATE FUNCTION shell_in(cstring) RETURNS public.shell_type LANGUAGE internal AS 'int4in'",
            "CREATE FUNCTION shell_out(public.shell_type) RETURNS cstring LANGUAGE internal AS 'int4out'",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        let name = FullName::with_schema("public", "shell_type");
        let reference = context
            .get_declared_type(&name)
            .unwrap()
            .type_reference()
            .cloned();
        assert!(context.get_type(&name).is_none());
        let identity = OwnershipIdentity::named(ObjectKind::TYPE, &name);
        assert_eq!(
            context.extension_membership(&identity),
            Some("shell_extension")
        );
        context.enter_extension_script("completion_extension", Some("public"));
        context = apply_ownership_sql(
            context,
            "CREATE TYPE public.shell_type (INPUT = shell_in, OUTPUT = shell_out, INTERNALLENGTH = 4)",
        )?
        .into_context();
        assert_eq!(
            context.get_type(&name).unwrap().type_reference(),
            reference.as_ref()
        );
        assert!(context.problems().is_empty());
        assert_eq!(
            context.extension_membership(&identity),
            Some("shell_extension")
        );
        assert_eq!(context.extension_membership_history(&identity).len(), 1);
        let function = FullName::with_schema("public", "shell_in");
        assert_eq!(
            context
                .get_function_impl(&function)
                .unwrap()
                .get_at(0)
                .unwrap()
                .ret_type(),
            &reference,
        );
        let output = FullName::with_schema("public", "shell_out");
        let definitions = context.routine_definitions(&output);
        let output_overload = definitions[0].overloads().get_at(0).unwrap();
        assert_eq!(
            output_overload.get_arg_def_at(0).unwrap().get_type(),
            reference.as_ref()
        );
        Ok(())
    }
}
