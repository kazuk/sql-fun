//! PostgreSQL text search dictionary declaration analysis and application.

use crate::{
    AstAndContextPair, StringSpan,
    ast::Literal,
    ast::text_search_declaration::{
        TextSearchDeclaration, TextSearchOption, TextSearchOptionValue,
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, SemAst, TextSearchTemplateDefinition,
        problem::CreationProblem,
        search_path::{
            DatabaseNameError, expand_search_path_inst, postgres_creation_schema,
            postgres_local_name,
        },
    },
};

/// Catalog identity of a PostgreSQL text search dictionary and its template.
#[derive(Clone, Debug)]
pub struct TextSearchDictionaryDefinition {
    name: FullName,
    template: FullName,
}

impl TextSearchDictionaryDefinition {
    /// Qualified dictionary name.
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// Qualified name of the resolved template.
    pub const fn template(&self) -> &FullName {
        &self.template
    }
}

/// Reason an analyzed text search dictionary cannot be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSearchDictionaryFailure {
    /// The requested database name is unknown.
    UnknownDatabase,
    /// The requested related database has no available schema.
    RelatedDatabase,
    /// The dictionary name is outside the current database.
    InvalidName,
    /// The creation schema could not be resolved.
    MissingCreationSchema,
    /// No TEMPLATE option was supplied.
    MissingTemplate,
    /// A TEMPLATE option did not contain a valid local name.
    InvalidTemplateName,
    /// A named template was absent from the catalog.
    TemplateNotFound,
    /// The selected template has no INIT callback and cannot accept options.
    OptionsWithoutInit,
    /// A dictionary already exists under the resolved name.
    AlreadyExists,
}

/// Analyzed PostgreSQL text search dictionary declaration.
#[derive(Clone, Debug)]
pub struct CreateTextSearchDictionary {
    name: Option<FullName>,
    template: Option<FullName>,
    definition: Result<TextSearchDictionaryDefinition, TextSearchDictionaryFailure>,
}

impl CreateTextSearchDictionary {
    /// Qualified creation target, when resolved.
    pub const fn name(&self) -> Option<&FullName> {
        self.name.as_ref()
    }

    /// Most recent successfully resolved TEMPLATE binding, if any.
    pub const fn template(&self) -> Option<&FullName> {
        self.template.as_ref()
    }

    /// The reason the declaration cannot be applied, if rejected.
    pub const fn failure(&self) -> Option<TextSearchDictionaryFailure> {
        match self.definition {
            Ok(_) => None,
            Err(failure) => Some(failure),
        }
    }

    /// The definition if the declaration is applicable.
    pub const fn definition(&self) -> Option<&TextSearchDictionaryDefinition> {
        match &self.definition {
            Ok(definition) => Some(definition),
            Err(_) => None,
        }
    }

    pub(crate) fn analyze(
        syntax: &TextSearchDeclaration,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut name = None;
        let mut template = None;
        let definition = match resolve_definition(syntax, context, &mut name, &mut template) {
            Ok(definition) => Ok(definition),
            Err((failure, problem)) => {
                context.report_analysis_problem(*problem)?;
                Err(failure)
            }
        };
        Ok(Self {
            name,
            template,
            definition,
        })
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &TextSearchDeclaration,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(syntax, &mut context)?;
        if let Some(definition) = statement.definition() {
            context = context.register_text_search_dictionary(definition);
        }
        Ok(AstAndContextPair::new(
            SemAst::CreateTextSearchDictionary(Box::new(statement)),
            context,
        ))
    }
}

#[cfg(test)]
mod applicability_tests {
    use crate::{
        ast::Dialect,
        context::{ParseContext, TextSearchDictionaryInfoRead},
        names::FullName,
        parse_statements,
        sem::{AnalysisProblem, SemAst, TextSearchDictionaryFailure, TypeReference},
        test_helpers::{
            TestParseContext, create_cache_context, schema_creation, search_path::search_path,
        },
    };

    #[test]
    fn rejected_dictionaries_retain_bindings_and_do_not_update_the_catalog()
    -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_search_path(&search_path(&["public"]));
        inventory.setup_type(TypeReference::from_master_type_name("internal"));
        let mut context = schema_creation::apply(
            schema_creation::child(inventory.freeze()),
            r#"
            CREATE FUNCTION lexize(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE TEXT SEARCH TEMPLATE original (LEXIZE = lexize);
            CREATE TEXT SEARCH TEMPLATE selected (LEXIZE = lexize);
            CREATE TEXT SEARCH DICTIONARY existing (TEMPLATE = original);
            "#,
        )?;
        assert!(context.problems().is_empty());
        let mut parsed = parse_statements(
            r#"
            CREATE TEXT SEARCH DICTIONARY existing (TEMPLATE = selected);
            CREATE TEXT SEARCH DICTIONARY with_options (TEMPLATE = selected, extra = true);
            CREATE TEXT SEARCH DICTIONARY unresolved (TEMPLATE = missing);
            CREATE TEXT SEARCH DICTIONARY partially_resolved
                (TEMPLATE = original, TEMPLATE = missing);
            CREATE TEXT SEARCH DICTIONARY with_options (TEMPLATE = selected);
            "#,
            Dialect::postgres(),
        );
        let expected = [
            (
                "existing",
                Some("selected"),
                Some(TextSearchDictionaryFailure::AlreadyExists),
                Some("original"),
            ),
            (
                "with_options",
                Some("selected"),
                Some(TextSearchDictionaryFailure::OptionsWithoutInit),
                None,
            ),
            (
                "unresolved",
                None,
                Some(TextSearchDictionaryFailure::TemplateNotFound),
                None,
            ),
            (
                "partially_resolved",
                Some("original"),
                Some(TextSearchDictionaryFailure::TemplateNotFound),
                None,
            ),
            ("with_options", Some("selected"), None, Some("selected")),
        ];
        let mut problem_count = 0;
        for (name, binding, failure, published) in expected {
            let syntax = parsed.next().expect("dictionary statement")?;
            let analyzed =
                SemAst::apply_ast_node(syntax.node(), context, &mut create_cache_context())?;
            let SemAst::CreateTextSearchDictionary(statement) = analyzed.ast() else {
                panic!("expected dictionary analysis");
            };
            let target = FullName::with_schema("public", name);
            assert_eq!(statement.name(), Some(&target));
            assert_eq!(
                statement.template(),
                binding
                    .map(|name| FullName::with_schema("public", name))
                    .as_ref()
            );
            assert_eq!(statement.failure(), failure);
            assert_eq!(statement.definition().is_none(), failure.is_some());

            context = analyzed.into_context();
            if failure.is_some() {
                problem_count += 1;
            }
            assert_eq!(context.problems().len(), problem_count);
            assert_eq!(
                context
                    .text_search_dictionary(&target)
                    .map(|definition| definition.template()),
                published
                    .map(|name| FullName::with_schema("public", name))
                    .as_ref(),
            );
        }
        assert!(parsed.next().is_none());
        assert!(context.problems()[0].creation_problem().is_some());
        assert!(matches!(
            context.problems()[1],
            AnalysisProblem::InvalidTextSearchDictionary(..)
        ));
        assert!(matches!(
            context.problems()[2],
            AnalysisProblem::InvalidTextSearchDictionary(..)
        ));
        Ok(())
    }
}

type Rejection = (TextSearchDictionaryFailure, Box<AnalysisProblem>);

fn resolve_definition(
    syntax: &TextSearchDeclaration,
    context: &impl ParseContext,
    resolved_name: &mut Option<FullName>,
    resolved_template: &mut Option<FullName>,
) -> Result<TextSearchDictionaryDefinition, Rejection> {
    let target = postgres_local_name(syntax.name().parts(), context).map_err(|error| {
        TextSearchDictionaryFailure::from_name_error(
            &error,
            TextSearchDictionaryFailure::InvalidName,
        )
        .problem(error.to_string(), None)
    })?;
    let schema = postgres_creation_schema(context, target.schema()).ok_or_else(|| {
        let message = target.schema().map_or_else(
            || "no schema selected for text search dictionary creation".to_owned(),
            |schema| format!("schema {schema} does not exist"),
        );
        TextSearchDictionaryFailure::MissingCreationSchema.problem(message, None)
    })?;
    let name = target.replace_schema(&schema);
    *resolved_name = Some(name.clone());
    let template = select_template(syntax, context, resolved_template)?;
    if context.text_search_dictionary(&name).is_some() {
        let span = context.get_span().copied().unwrap_or_default();
        return Err((
            TextSearchDictionaryFailure::AlreadyExists,
            Box::new(AnalysisProblem::creation(CreationProblem::named(
                &name,
                "text search dictionary",
                &span,
            ))),
        ));
    }
    Ok(TextSearchDictionaryDefinition {
        name,
        template: template.name().clone(),
    })
}

fn select_template<'a>(
    syntax: &TextSearchDeclaration,
    context: &'a impl ParseContext,
    resolved_template: &mut Option<FullName>,
) -> Result<&'a TextSearchTemplateDefinition, Rejection> {
    let mut template = None;
    let mut first_setting = None;
    for option in syntax.options() {
        if option.key() == "template" {
            let binding = resolve_template(option, context)?;
            *resolved_template = Some(binding.name().clone());
            template = Some(binding);
        } else {
            first_setting.get_or_insert(option);
        }
    }
    let template = template.ok_or_else(|| {
        TextSearchDictionaryFailure::MissingTemplate
            .problem("text search template is required", None)
    })?;
    if let Some(setting) = first_setting
        && template.init().is_none()
    {
        return Err(TextSearchDictionaryFailure::OptionsWithoutInit.problem(
            format!(
                "text search template {} does not accept options",
                template.name()
            ),
            setting.location(),
        ));
    }
    Ok(template)
}

fn resolve_template<'a>(
    option: &TextSearchOption,
    context: &'a impl ParseContext,
) -> Result<&'a TextSearchTemplateDefinition, Rejection> {
    let name = match option.value() {
        Some(TextSearchOptionValue::Name(name)) => postgres_local_name(name.parts(), context),
        Some(TextSearchOptionValue::Literal(Literal::String(name))) => {
            Ok(FullName::from_local_name(name))
        }
        _ => Err(DatabaseNameError::InvalidName),
    }
    .map_err(|error| {
        TextSearchDictionaryFailure::from_name_error(
            &error,
            TextSearchDictionaryFailure::InvalidTemplateName,
        )
        .problem(error.to_string(), option.location())
    })?;
    expand_search_path_inst(context, &name, |key| context.text_search_template(key)).ok_or_else(
        || {
            TextSearchDictionaryFailure::TemplateNotFound.problem(
                format!("text search template {name} does not exist"),
                option.location(),
            )
        },
    )
}

impl TextSearchDictionaryFailure {
    const fn from_name_error(error: &DatabaseNameError<'_>, invalid: Self) -> Self {
        match error {
            DatabaseNameError::InvalidName => invalid,
            DatabaseNameError::Unknown(_) => Self::UnknownDatabase,
            DatabaseNameError::Related(_) => Self::RelatedDatabase,
        }
    }

    fn problem(self, message: impl Into<String>, location: Option<StringSpan>) -> Rejection {
        (
            self,
            Box::new(AnalysisProblem::InvalidTextSearchDictionary(
                message.into(),
                location.as_ref().map(Into::into),
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        PostgresDialect, SqlFileSnapshot,
        context::{BaseParseContext, ParseContext, TextSearchDictionaryInfoRead},
        names::{FullName, SchemaName},
        sem::TypeReference,
        test_helpers::{
            TestParseContext, schema_content_hash::schema_file, schema_creation,
            search_path::search_path,
        },
    };

    #[test]
    fn resolves_templates_and_preserves_frozen_dictionary_definitions() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_search_path(&search_path(&["public"]));
        inventory.setup_type(TypeReference::from_master_type_name("internal"));
        let context = schema_creation::apply(
            schema_creation::child(inventory.freeze()),
            r#"
            CREATE SCHEMA "Search";
            CREATE FUNCTION public.lexize(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE TEXT SEARCH TEMPLATE public."Chosen" (LEXIZE = lexize);
            CREATE TEXT SEARCH TEMPLATE "Search"."Chosen" (LEXIZE = lexize);
            CREATE TEXT SEARCH DICTIONARY public.existing
                (TEMPLATE = public."Chosen", TEMPLATE = "Search"."Chosen");
            "#,
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let parent = context.freeze();

        let child = schema_creation::apply(
            schema_creation::child(parent.clone()),
            r#"
            CREATE TEXT SEARCH DICTIONARY public.existing (TEMPLATE = 'Chosen');
            CREATE TEXT SEARCH DICTIONARY public.missing
                (TEMPLATE = absent, TEMPLATE = 'Chosen');
            CREATE TEXT SEARCH DICTIONARY "Search".distinct_name (TEMPLATE = 'Chosen');
            "#,
        )?;
        assert_eq!(child.problems().len(), 2);
        let existing = FullName::with_schema("public", "existing");
        assert_eq!(
            child.text_search_dictionary(&existing).unwrap().template(),
            &FullName::with_schema("Search", "Chosen")
        );
        assert!(
            child
                .text_search_dictionary(&FullName::with_schema("public", "missing"))
                .is_none()
        );
        let distinct = FullName::with_schema("Search", "distinct_name");
        assert!(parent.text_search_dictionary(&distinct).is_none());
        assert_eq!(
            child
                .freeze()
                .text_search_dictionary(&distinct)
                .unwrap()
                .template(),
            &FullName::with_schema("public", "Chosen")
        );
        Ok(())
    }

    #[test]
    fn extension_replay_resolves_and_publishes_dictionary() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_search_path(&search_path(&["public"]));
        inventory.setup_type(TypeReference::from_master_type_name("internal"));
        let base = schema_creation::apply(
            schema_creation::child(inventory.freeze()),
            "CREATE SCHEMA extension_schema;",
        )?
        .freeze();
        let file = schema_file(
            r#"
            CREATE FUNCTION lexize(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE TEXT SEARCH TEMPLATE template (LEXIZE = lexize);
            CREATE TEXT SEARCH DICTIONARY dictionary (TEMPLATE = template);
            "#,
        );
        let snapshot = SqlFileSnapshot::read(file.path())?;
        let parsed = PostgresDialect::parse_extension_snapshot(
            &snapshot,
            &base,
            base.get_args(),
            base.metadata(),
            &[SchemaName::from("extension_schema")],
            ("example", Some("extension_schema")),
        )?;
        assert!(parsed.problems().is_empty(), "{:?}", parsed.problems());
        let dictionary_name = FullName::with_schema("extension_schema", "dictionary");
        assert!(base.text_search_dictionary(&dictionary_name).is_none());
        let dictionary = parsed
            .base_context()
            .text_search_dictionary(&dictionary_name)
            .unwrap();
        assert_eq!(
            dictionary.template(),
            &FullName::with_schema("extension_schema", "template")
        );
        Ok(())
    }
}
