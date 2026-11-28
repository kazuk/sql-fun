//! PostgreSQL text search template declaration analysis and application.

use crate::{
    AstAndContextPair, StringSpan,
    ast::{
        Literal, Name,
        text_search_declaration::{
            TextSearchDeclaration as Syntax, TextSearchOption, TextSearchOptionValue,
        },
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, SemAst, TypeReference,
        search_path::{DatabaseNameError, postgres_creation_schema, postgres_local_name},
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

/// Reason a text search template declaration cannot be applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextSearchTemplateFailure {
    /// The requested database name is unknown.
    UnknownDatabase,
    /// The requested related database has no available schema.
    RelatedDatabase,
    /// The written target cannot be used in this database.
    InvalidTarget,
    /// The requested schema does not exist or the search path has no creation schema.
    MissingSchema,
    /// The option key is unsupported.
    UnknownOption,
    /// The callback value is not a usable routine name.
    InvalidCallbackName,
    /// No routine matched the required input signature.
    MissingCallback,
    /// The resolved routine does not return `internal`.
    WrongReturnType,
    /// The required LEXIZE option was omitted.
    MissingLexize,
    /// The target already names a registered template.
    DuplicateTemplate,
}

/// Completed analysis of one written template option.
#[derive(Clone, Debug)]
pub struct TextSearchTemplateOption {
    key: String,
    requested: Option<Vec<String>>,
    resolved: Option<FullName>,
    failure: Option<TextSearchTemplateFailure>,
}

impl TextSearchTemplateOption {
    /// Written option key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Written routine name components, when the value names a routine.
    pub fn requested(&self) -> Option<&[String]> {
        self.requested.as_deref()
    }

    /// Exact routine identity, including when its return type is invalid.
    pub const fn resolved(&self) -> Option<&FullName> {
        self.resolved.as_ref()
    }

    /// Validation outcome for this option.
    pub const fn failure(&self) -> Option<TextSearchTemplateFailure> {
        self.failure
    }
}

/// Analyzed PostgreSQL text search template declaration.
#[derive(Clone, Debug)]
pub struct CreateTextSearchTemplate {
    requested: Vec<String>,
    name: Option<FullName>,
    options: Vec<TextSearchTemplateOption>,
    applicability: Result<TextSearchTemplateDefinition, TextSearchTemplateFailure>,
}

impl CreateTextSearchTemplate {
    /// Written template name components.
    pub fn requested(&self) -> &[String] {
        &self.requested
    }

    /// Qualified target when namespace resolution succeeded.
    pub const fn name(&self) -> Option<&FullName> {
        self.name.as_ref()
    }

    /// Completed option outcomes in source order.
    pub fn options(&self) -> &[TextSearchTemplateOption] {
        &self.options
    }

    /// Catalog applicability or the first failure in PostgreSQL validation order.
    pub const fn applicability(
        &self,
    ) -> &Result<TextSearchTemplateDefinition, TextSearchTemplateFailure> {
        &self.applicability
    }

    /// The definition if the declaration is applicable.
    pub const fn definition(&self) -> Option<&TextSearchTemplateDefinition> {
        match &self.applicability {
            Ok(definition) => Some(definition),
            Err(_) => None,
        }
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let target = creation_name(syntax.name(), context);
        let mut first_error = None;
        let mut options = Vec::new();
        let mut init = None;
        let mut lexize = None;
        for option in syntax.options() {
            let (outcome, error) = analyze_option(option, context);
            if first_error.is_none() {
                first_error = error;
            }
            if outcome.failure().is_none() {
                match outcome.key() {
                    "init" => init = outcome.resolved().cloned(),
                    "lexize" => lexize = outcome.resolved().cloned(),
                    _ => {}
                }
            }
            options.push(outcome);
        }

        let name = target.as_ref().ok().cloned();
        let applicability = target.and_then(|name| match first_error {
            Some(error) => Err(error),
            None => finish_definition(name, init, lexize, context),
        });
        let applicability = match applicability {
            Ok(definition) => Ok(definition),
            Err(error) => {
                context.report_analysis_problem(AnalysisProblem::InvalidTextSearchTemplate(
                    error.message,
                    error.location.as_ref().map(Into::into),
                ))?;
                Err(error.failure)
            }
        };
        Ok(Self {
            requested: syntax.name().parts().to_vec(),
            name,
            options,
            applicability,
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
        sem::{SemAst, TextSearchTemplateFailure, TypeReference},
        test_helpers::{
            TestParseContext, ownership::apply_ownership_sql, schema_creation,
            search_path::search_path,
        },
    };

    #[test]
    fn retains_last_binding_in_frozen_layers() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_search_path(&search_path(&["public"]));
        inventory.setup_type(TypeReference::from_master_type_name("internal"));
        inventory.setup_type(TypeReference::from_master_type_name("int4"));
        let context = schema_creation::apply(
            schema_creation::child(inventory.freeze()),
            r#"
            CREATE FUNCTION public.first_callback(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE FUNCTION public."Second"(internal, internal, internal, internal)
                RETURNS internal AS 'fixture' LANGUAGE C;
            CREATE FUNCTION public.bad_callback(internal, internal, internal, internal)
                RETURNS int4 AS 'fixture' LANGUAGE C;
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

        let duplicate = apply_ownership_sql(
            schema_creation::child(parent.clone()),
            "CREATE TEXT SEARCH TEMPLATE public.demo (LEXIZE = first_callback)",
        )?;
        let SemAst::CreateTextSearchTemplate(statement) = duplicate.ast() else {
            panic!("expected a text search template");
        };
        assert_eq!(statement.name(), Some(&name));
        assert_eq!(
            statement.options()[0].resolved(),
            Some(&FullName::with_schema("public", "first_callback"))
        );
        assert_eq!(
            statement.applicability().as_ref().unwrap_err(),
            &TextSearchTemplateFailure::DuplicateTemplate
        );

        let invalid_name = FullName::with_schema("public", "invalid");
        let invalid = apply_ownership_sql(
            duplicate.into_context(),
            "CREATE TEXT SEARCH TEMPLATE public.invalid (LEXIZE = missing, LEXIZE = first_callback)",
        )?;
        let SemAst::CreateTextSearchTemplate(statement) = invalid.ast() else {
            panic!("expected a text search template");
        };
        assert_eq!(statement.name(), Some(&invalid_name));
        assert_eq!(
            statement.options()[0].failure(),
            Some(TextSearchTemplateFailure::MissingCallback)
        );
        assert_eq!(
            statement.options()[1].resolved(),
            Some(&FullName::with_schema("public", "first_callback"))
        );
        assert_eq!(
            statement.applicability().as_ref().unwrap_err(),
            &TextSearchTemplateFailure::MissingCallback
        );

        let bad_return = apply_ownership_sql(
            invalid.into_context(),
            "CREATE TEXT SEARCH TEMPLATE public.invalid (LEXIZE = bad_callback)",
        )?;
        let SemAst::CreateTextSearchTemplate(statement) = bad_return.ast() else {
            panic!("expected a text search template");
        };
        assert_eq!(
            statement.options()[0].resolved(),
            Some(&FullName::with_schema("public", "bad_callback"))
        );
        assert_eq!(
            statement.applicability().as_ref().unwrap_err(),
            &TextSearchTemplateFailure::WrongReturnType
        );
        let child = bad_return.into_context();
        assert_eq!(child.problems().len(), 3);
        assert!(child.text_search_template(&invalid_name).is_none());
        assert_eq!(
            child.text_search_template(&name).unwrap().lexize(),
            entry.lexize()
        );
        Ok(())
    }
}

use TextSearchTemplateFailure as Failure;

struct ValidationError {
    failure: Failure,
    message: String,
    location: Option<StringSpan>,
}

fn finish_definition(
    name: FullName,
    init: Option<FullName>,
    lexize: Option<FullName>,
    context: &impl ParseContext,
) -> Result<TextSearchTemplateDefinition, ValidationError> {
    let lexize = lexize.ok_or_else(|| ValidationError {
        failure: Failure::MissingLexize,
        message: "text search template requires LEXIZE".to_owned(),
        location: None,
    })?;
    if context.text_search_template(&name).is_some() {
        return Err(ValidationError {
            failure: Failure::DuplicateTemplate,
            message: format!("text search template {name} already exists"),
            location: None,
        });
    }
    Ok(TextSearchTemplateDefinition { name, init, lexize })
}

fn analyze_option(
    option: &TextSearchOption,
    context: &impl ParseContext,
) -> (TextSearchTemplateOption, Option<ValidationError>) {
    let (requested, callback) = match option.value() {
        Some(TextSearchOptionValue::Name(name)) => (
            Some(name.parts().to_vec()),
            postgres_local_name(name.parts(), context).map(Some),
        ),
        Some(TextSearchOptionValue::Literal(Literal::String(name))) => (
            Some(vec![name.clone()]),
            Ok(Some(FullName::from_local_name(name))),
        ),
        _ => (None, Ok(None)),
    };
    let mut option_outcome = TextSearchTemplateOption {
        key: option.key().to_owned(),
        requested,
        resolved: None,
        failure: None,
    };
    let error = bind_callback(&mut option_outcome, callback, context)
        .err()
        .map(|(failure, message)| ValidationError {
            failure,
            message,
            location: option.location(),
        });
    option_outcome.failure = error.as_ref().map(|error| error.failure);
    (option_outcome, error)
}

fn bind_callback(
    option: &mut TextSearchTemplateOption,
    callback: Result<Option<FullName>, DatabaseNameError<'_>>,
    context: &impl ParseContext,
) -> Result<(), (Failure, String)> {
    let key = option.key();
    let arity = match key {
        "init" => 1,
        "lexize" => 4,
        _ => {
            return Err((
                Failure::UnknownOption,
                format!("unrecognized text search template option {key:?}"),
            ));
        }
    };
    let callback = callback.map_err(|error| {
        (
            Failure::from_name_error(&error, Failure::InvalidCallbackName),
            error.to_string(),
        )
    })?;
    let callback = callback.ok_or_else(|| {
        (
            Failure::InvalidCallbackName,
            format!("{key} requires a function name"),
        )
    })?;
    let internal = TypeReference::from_master_type_name("internal");
    let arguments = vec![Some(internal.clone()); arity];
    let function = context
        .find_exact_function(&callback, &arguments)
        .ok_or_else(|| {
            (
                Failure::MissingCallback,
                format!("{key} function {callback} must exist with {arity} internal arguments"),
            )
        })?;
    let failure = if function.overload().ret_type().as_ref() == Some(&internal) {
        Ok(())
    } else {
        Err((
            Failure::WrongReturnType,
            format!("{key} function {callback} must return internal"),
        ))
    };
    option.resolved = Some(function.name().clone());
    failure
}

fn creation_name(written: &Name, context: &impl ParseContext) -> Result<FullName, ValidationError> {
    let name = postgres_local_name(written.parts(), context).map_err(|error| ValidationError {
        failure: Failure::from_name_error(&error, Failure::InvalidTarget),
        message: error.to_string(),
        location: None,
    })?;
    if let Some(schema) = postgres_creation_schema(context, name.schema()) {
        return Ok(name.replace_schema(&schema));
    }
    Err(ValidationError {
        failure: Failure::MissingSchema,
        message: name.schema().map_or_else(
            || "no schema selected for text search template creation".to_owned(),
            |schema| format!("schema {schema} does not exist"),
        ),
        location: None,
    })
}

impl TextSearchTemplateFailure {
    const fn from_name_error(error: &DatabaseNameError<'_>, invalid: Self) -> Self {
        match error {
            DatabaseNameError::InvalidName => invalid,
            DatabaseNameError::Unknown(_) => Self::UnknownDatabase,
            DatabaseNameError::Related(_) => Self::RelatedDatabase,
        }
    }
}
