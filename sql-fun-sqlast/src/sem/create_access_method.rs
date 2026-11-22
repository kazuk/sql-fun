//! PostgreSQL access-method declaration analysis and application.

use crate::{
    AstAndContextPair,
    ast::create_access_method::CreateAccessMethod as Syntax,
    context::ParseContext,
    sem::{AccessMethodKind, AnalysisError, AnalysisProblem, SemAst},
};

/// A database-wide access-method definition.
#[derive(Clone, Debug)]
pub struct AccessMethodDefinition {
    name: String,
    kind: AccessMethodKind,
    handler: Vec<String>,
}

impl AccessMethodDefinition {
    /// Normalized SQL identifier and catalog key.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The declared access-method kind.
    #[must_use]
    pub const fn kind(&self) -> AccessMethodKind {
        self.kind
    }

    /// Written handler identifier components, including any qualification.
    #[must_use]
    pub fn handler(&self) -> &[String] {
        &self.handler
    }
}

/// Analyzed PostgreSQL access-method declaration.
#[derive(Clone, Debug)]
pub struct CreateAccessMethod {
    definition: AccessMethodDefinition,
    applicable: bool,
}

impl CreateAccessMethod {
    /// The definition supplied by the declaration.
    #[must_use]
    pub const fn definition(&self) -> &AccessMethodDefinition {
        &self.definition
    }

    /// Whether the declaration can be applied to the catalog.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        self.applicable
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let definition = AccessMethodDefinition {
            name: syntax.name().to_owned(),
            kind: syntax.kind(),
            handler: syntax.handler().to_vec(),
        };
        let applicable = context.access_method(definition.name()).is_none();
        if !applicable {
            context.report_analysis_problem(AnalysisProblem::DuplicateAccessMethod(
                definition.name().to_owned(),
            ))?;
        }
        Ok(Self {
            definition,
            applicable,
        })
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &Syntax,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(syntax, &mut context)?;
        let context = if statement.is_applicable() {
            context.register_access_method(statement.definition())
        } else {
            context
        };
        Ok(AstAndContextPair::new(
            SemAst::CreateAccessMethod(Box::new(statement)),
            context,
        ))
    }
}

#[cfg(test)]
mod test_analyze_and_apply {
    use crate::{
        PostgresDialect, SqlFileSnapshot,
        context::{AccessMethodInfoRead, BaseParseContext, ParseContext},
        names::SchemaName,
        sem::{AccessMethodKind, SemAst},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_content_hash::schema_file,
            schema_creation,
        },
    };

    #[test]
    fn freezes_registration_and_rejects_inherited_duplicate() -> testresult::TestResult {
        let parent = schema_creation::postgres(
            r#"CREATE ACCESS METHOD "App Method" TYPE TABLE HANDLER "App"."Table Handler""#,
        )?
        .freeze();
        let result = apply_ownership_sql(
            schema_creation::child(parent),
            r#"CREATE ACCESS METHOD "App Method" TYPE INDEX HANDLER other_handler"#,
        )?;
        let SemAst::CreateAccessMethod(statement) = result.ast() else {
            panic!("expected an analyzed access-method declaration");
        };
        assert!(!statement.is_applicable());
        assert_eq!(statement.definition().kind(), AccessMethodKind::Index);
        assert_eq!(statement.definition().handler(), ["other_handler"]);

        let child = result.into_context();
        let existing = child.access_method("App Method").unwrap();
        assert_eq!(existing.kind(), AccessMethodKind::Table);
        assert_eq!(existing.handler(), ["App", "Table Handler"]);
        assert_eq!(child.problems()[0].code(), "duplicate_access_method");

        Ok(())
    }

    #[test]
    fn extension_replay_publishes_bloom() -> testresult::TestResult {
        let input = ownership_context().freeze();
        let file = schema_file("CREATE ACCESS METHOD bloom TYPE INDEX HANDLER blhandler;");
        let snapshot = SqlFileSnapshot::read(file.path())?;
        let parsed = PostgresDialect::parse_extension_snapshot(
            &snapshot,
            &input,
            input.get_args(),
            input.metadata(),
            &[SchemaName::from("public")],
            ("bloom", Some("public")),
        )?;
        assert!(parsed.problems().is_empty());
        let method = parsed.base_context().access_method("bloom").unwrap();
        assert_eq!(method.kind(), AccessMethodKind::Index);
        assert_eq!(method.handler(), ["blhandler"]);
        Ok(())
    }
}
