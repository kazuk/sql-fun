//! PostgreSQL text search template declaration analysis and application.

use crate::{
    AstAndContextPair, StringSpan,
    ast::{
        Literal, Name,
        create_text_search_template::{CreateTextSearchTemplate as Syntax, TemplateOptionValue},
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, SemAst, TypeReference,
        search_path::postgres_creation_schema,
    },
};

/// Catalog identity and resolved routines for a PostgreSQL text search template.
#[derive(Clone, Debug)]
pub struct TextSearchTemplateDefinition {
    name: FullName,
    init: Option<FullName>,
    lexize: FullName,
}

impl TextSearchTemplateDefinition {
    /// Qualified template name.
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// Qualified initializer name, if specified. Its signature is fixed.
    pub const fn init(&self) -> Option<&FullName> {
        self.init.as_ref()
    }

    /// Qualified lexizer name. Its signature is fixed.
    pub const fn lexize(&self) -> &FullName {
        &self.lexize
    }
}

/// Analyzed PostgreSQL text search template declaration.
#[derive(Clone, Debug)]
pub struct CreateTextSearchTemplate {
    definition: Option<TextSearchTemplateDefinition>,
}

impl CreateTextSearchTemplate {
    /// The definition if the declaration is applicable.
    pub const fn definition(&self) -> Option<&TextSearchTemplateDefinition> {
        self.definition.as_ref()
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            definition: analyze_definition(syntax, context)?,
        })
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &Syntax,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(syntax, &mut context)?;
        if let Some(definition) = statement.definition() {
            context = context.register_text_search_template(definition);
        }
        Ok(AstAndContextPair::new(
            SemAst::CreateTextSearchTemplate(Box::new(statement)),
            context,
        ))
    }
}

#[cfg(test)]
mod test_catalog_application {
    use crate::{
        context::{ParseContext, TextSearchTemplateInfoRead},
        names::FullName,
        sem::TypeReference,
        test_helpers::{TestParseContext, schema_creation, search_path::search_path},
    };

    #[test]
    fn retains_last_binding_in_frozen_layers() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_search_path(&search_path(&["public"]));
        inventory.setup_type(TypeReference::from_master_type_name("internal"));
        let context = schema_creation::apply(
            schema_creation::child(inventory.freeze()),
            r#"
            CREATE FUNCTION public.first_callback(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE FUNCTION public."Second"(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE TEXT SEARCH TEMPLATE public.demo
                (LEXIZE = first_callback, LEXIZE = 'Second');
            "#,
        )?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());

        let name = FullName::with_schema("public", "demo");
        let parent = context.freeze();
        let entry = parent.text_search_template(&name).unwrap();
        assert!(entry.init().is_none());
        assert_eq!(entry.lexize(), &FullName::with_schema("public", "Second"));

        let child = schema_creation::apply(
            schema_creation::child(parent.clone()),
            "CREATE TEXT SEARCH TEMPLATE public.demo (LEXIZE = first_callback);
             CREATE TEXT SEARCH TEMPLATE public.invalid
                 (LEXIZE = missing, LEXIZE = first_callback);",
        )?;
        assert_eq!(child.problems().len(), 2);
        assert!(
            child
                .text_search_template(&FullName::with_schema("public", "invalid"))
                .is_none()
        );
        assert_eq!(
            child.text_search_template(&name).unwrap().lexize(),
            entry.lexize()
        );
        Ok(())
    }
}

fn analyze_definition(
    syntax: &Syntax,
    context: &mut impl ParseContext,
) -> Result<Option<TextSearchTemplateDefinition>, AnalysisError> {
    let Some(name) = creation_name(syntax, context)? else {
        return Ok(None);
    };
    let mut init = None;
    let mut lexize = None;
    for option in &syntax.options {
        let (arity, binding) = match option.key.as_str() {
            "init" => (1, &mut init),
            "lexize" => (4, &mut lexize),
            key => {
                return reject(
                    context,
                    format!("unrecognized text search template option {key:?}"),
                    option.location,
                );
            }
        };
        let Some(callback) = callback_name(option.value.as_ref(), context) else {
            return reject(
                context,
                format!("{} requires a function name", option.key),
                option.location,
            );
        };
        let internal = TypeReference::from_master_type_name("internal");
        let arguments = vec![Some(internal.clone()); arity];
        let Some(function) = context.find_exact_function(&callback, &arguments) else {
            return reject(
                context,
                format!(
                    "{} function {callback} must exist with {arity} internal arguments",
                    option.key
                ),
                option.location,
            );
        };
        if function.overload().ret_type().as_ref() != Some(&internal) {
            return reject(
                context,
                format!("{} function {callback} must return internal", option.key),
                option.location,
            );
        }
        *binding = Some(function.name().clone());
    }
    let Some(lexize) = lexize else {
        return reject(context, "text search template requires LEXIZE", None);
    };
    if context.text_search_template(&name).is_some() {
        return reject(
            context,
            format!("text search template {name} already exists"),
            None,
        );
    }
    Ok(Some(TextSearchTemplateDefinition { name, init, lexize }))
}

fn creation_name(
    syntax: &Syntax,
    context: &mut impl ParseContext,
) -> Result<Option<FullName>, AnalysisError> {
    let Some(name) = local_name(&syntax.name, context) else {
        return reject(
            context,
            "invalid text search template name for this database",
            None,
        );
    };
    if let Some(schema) = postgres_creation_schema(context, name.schema()) {
        return Ok(Some(name.replace_schema(&schema)));
    }
    let message = name.schema().map_or_else(
        || "no schema selected for text search template creation".to_owned(),
        |schema| format!("schema {schema} does not exist"),
    );
    reject(context, message, None)
}

fn callback_name(
    value: Option<&TemplateOptionValue>,
    context: &impl ParseContext,
) -> Option<FullName> {
    match value {
        Some(TemplateOptionValue::Name(name)) => local_name(name, context),
        Some(TemplateOptionValue::Literal(Literal::String(name))) => {
            Some(FullName::from_local_name(name))
        }
        _ => None,
    }
}

fn local_name(name: &Name, context: &impl ParseContext) -> Option<FullName> {
    match name.parts() {
        [local] => Some(FullName::from_local_name(local)),
        [schema, local] => Some(FullName::with_schema(schema, local)),
        [database, schema, local] if context.metadata().database_name() == Some(database) => {
            Some(FullName::with_schema(schema, local))
        }
        _ => None,
    }
}

fn reject<T>(
    context: &mut impl ParseContext,
    message: impl Into<String>,
    span: Option<StringSpan>,
) -> Result<Option<T>, AnalysisError> {
    context.report_analysis_problem(AnalysisProblem::InvalidTextSearchTemplate(
        message.into(),
        span.as_ref().map(Into::into),
    ))?;
    Ok(None)
}
