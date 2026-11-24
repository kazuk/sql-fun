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
        search_path::{expand_search_path_inst, postgres_creation_schema, postgres_local_name},
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

/// Analyzed PostgreSQL text search dictionary declaration.
#[derive(Clone, Debug)]
pub struct CreateTextSearchDictionary {
    definition: Option<TextSearchDictionaryDefinition>,
}

impl CreateTextSearchDictionary {
    /// The definition if the declaration is applicable.
    pub const fn definition(&self) -> Option<&TextSearchDictionaryDefinition> {
        self.definition.as_ref()
    }

    pub(crate) fn analyze(
        syntax: &TextSearchDeclaration,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let definition = match resolve_definition(syntax, context) {
            Ok(definition) => Some(definition),
            Err(problem) => {
                context.report_analysis_problem(*problem)?;
                None
            }
        };
        Ok(Self { definition })
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

fn resolve_definition(
    syntax: &TextSearchDeclaration,
    context: &impl ParseContext,
) -> Result<TextSearchDictionaryDefinition, Box<AnalysisProblem>> {
    let target = postgres_local_name(syntax.name(), context).ok_or_else(|| {
        invalid(
            "invalid text search dictionary name for this database",
            None,
        )
    })?;
    let schema = postgres_creation_schema(context, target.schema()).ok_or_else(|| {
        let message = target.schema().map_or_else(
            || "no schema selected for text search dictionary creation".to_owned(),
            |schema| format!("schema {schema} does not exist"),
        );
        invalid(message, None)
    })?;
    let name = target.replace_schema(&schema);
    let template = select_template(syntax, context)?;
    if context.text_search_dictionary(&name).is_some() {
        let span = context.get_span().copied().unwrap_or_default();
        return Err(Box::new(AnalysisProblem::creation(CreationProblem::named(
            &name,
            "text search dictionary",
            &span,
        ))));
    }
    Ok(TextSearchDictionaryDefinition {
        name,
        template: template.name().clone(),
    })
}

fn select_template<'a>(
    syntax: &TextSearchDeclaration,
    context: &'a impl ParseContext,
) -> Result<&'a TextSearchTemplateDefinition, Box<AnalysisProblem>> {
    let mut template = None;
    let mut first_setting = None;
    for option in syntax.options() {
        if option.key() == "template" {
            template = Some(resolve_template(option, context)?);
        } else {
            first_setting.get_or_insert(option);
        }
    }
    let template = template.ok_or_else(|| invalid("text search template is required", None))?;
    if let Some(setting) = first_setting
        && template.init().is_none()
    {
        return Err(invalid(
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
) -> Result<&'a TextSearchTemplateDefinition, Box<AnalysisProblem>> {
    let name = match option.value() {
        Some(TextSearchOptionValue::Name(name)) => postgres_local_name(name, context),
        Some(TextSearchOptionValue::Literal(Literal::String(name))) => {
            Some(FullName::from_local_name(name))
        }
        _ => None,
    }
    .ok_or_else(|| invalid("TEMPLATE requires a valid template name", option.location()))?;
    expand_search_path_inst(context, &name, |key| context.text_search_template(key)).ok_or_else(
        || {
            invalid(
                format!("text search template {name} does not exist"),
                option.location(),
            )
        },
    )
}

fn invalid(message: impl Into<String>, location: Option<StringSpan>) -> Box<AnalysisProblem> {
    Box::new(AnalysisProblem::InvalidTextSearchDictionary(
        message.into(),
        location.as_ref().map(Into::into),
    ))
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
