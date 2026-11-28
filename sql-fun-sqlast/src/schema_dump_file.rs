use crate::{
    context::{FrozenBaseContext, TypeInfoRead},
    names::FullName,
    sem::{AnalysisError, AnalysisProblem, SemAst, TypeDefinition, TypeReference},
};

/// Error for reading schema dump
#[derive(Debug, thiserror::Error)]
pub enum ParseDumpFileError {
    /// IO Error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Syntax Error
    #[error(transparent)]
    Syn(#[from] ::pg_query::Error),

    /// Fatal semantic analysis error.
    ///
    /// Recoverable analysis problems remain on a successful [`ParsedSchemaDump`].
    #[error(transparent)]
    Sem(#[from] AnalysisError),

    /// Syntax Error
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    SqliteSyn(#[from] ::turso_parser::error::Error),
}

impl ParseDumpFileError {
    /// Whether retrying file access may succeed without changing SQL contents.
    pub const fn is_io(&self) -> bool {
        matches!(self, Self::Io(_))
    }
}

/// A loaded schema file and its immutable catalog.
///
/// The semantic statements and recoverable problems belong to this file's
/// result. Chaining [`Self::base_context`] shares only its completed catalog,
/// including the effective search path and final source provenance.
#[derive(Debug)]
pub struct ParsedSchemaDump {
    _sem_results: Vec<SemAst>,
    base: FrozenBaseContext,
    problems: Vec<AnalysisProblem>,
    presentations: Vec<Option<::sql_fun_core::DiagnosticPresentation>>,
}

impl ParsedSchemaDump {
    pub(crate) const fn new(
        sem_results: Vec<SemAst>,
        base: FrozenBaseContext,
        problems: Vec<AnalysisProblem>,
        presentations: Vec<Option<::sql_fun_core::DiagnosticPresentation>>,
    ) -> Self {
        Self {
            _sem_results: sem_results,
            base,
            problems,
            presentations,
        }
    }

    /// The completed catalog, suitable as the explicit base for another file.
    ///
    /// Cloning this handle keeps the catalog alive independently of this parse
    /// result, without retaining its semantic statements or problems.
    pub const fn base_context(&self) -> &FrozenBaseContext {
        &self.base
    }

    /// Presentation context for each problem, in the same reporting order.
    ///
    /// An absent entry means the context was not given statement source text.
    pub fn diagnostic_presentations(&self) -> &[Option<::sql_fun_core::DiagnosticPresentation>] {
        &self.presentations
    }

    /// Recoverable problems from this file, in reporting order.
    ///
    /// Problems from earlier files stay on their own parse results.
    pub fn problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }
}

#[cfg(test)]
mod test_extension_catalog {
    use crate::{
        PostgresDialect, SqlFileSnapshot,
        context::{BaseParseContext, OwnershipInfoRead, ParseContext},
        sem::SemAst,
        test_helpers::ownership::ownership_context,
    };
    use std::path::Path;

    // Recover the timed-out bundled PL/pgSQL catalog test from commit
    // d3946324970cac9e1205fa6d7c0891cbce54c65a^, the parent of its removal commit.

    #[test]
    fn duplicate_function_preserves_the_original_catalog_definition() -> testresult::TestResult {
        let environment = ownership_context();
        let catalog = ownership_context().freeze();
        let source = Path::new("original.sql");
        let original = SqlFileSnapshot::from_text(
            source,
            "CREATE FUNCTION public.probe() RETURNS integer LANGUAGE sql AS 'SELECT 1';".into(),
        );
        let original = PostgresDialect::parse_schema_snapshot(
            &original,
            &catalog,
            environment.get_args(),
            environment.metadata(),
        )?;
        assert!(original.problems().is_empty());
        let duplicate = SqlFileSnapshot::from_text(
            Path::new("duplicate.sql"),
            "CREATE FUNCTION public.probe() RETURNS text LANGUAGE sql AS 'SELECT NULL::text';"
                .into(),
        );
        let duplicate = PostgresDialect::parse_schema_snapshot(
            &duplicate,
            original.base_context(),
            environment.get_args(),
            environment.metadata(),
        )?;
        let [SemAst::CreateFunction(function)] = duplicate._sem_results.as_slice() else {
            panic!("expected a semantic function declaration");
        };
        assert!(!function.is_applicable());
        assert_eq!(duplicate.problems().len(), 1);
        let definitions = duplicate
            .base_context()
            .routine_definitions(function.name());
        let [effective] = definitions.as_slice() else {
            panic!("expected one effective function definition");
        };
        assert_eq!(effective.source().unwrap().source_path(), source);
        Ok(())
    }
}

impl TypeInfoRead for ParsedSchemaDump {
    fn type_references(&self) -> Vec<TypeReference> {
        self.base_context().type_references()
    }

    fn builtin_type_ordered(&self, name: &FullName) -> Option<bool> {
        self.base_context().builtin_type_ordered(name)
    }

    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.base_context().get_type_impl(key)
    }

    fn array_type_by_catalog_name(&self, key: &FullName) -> Option<TypeReference> {
        self.base_context().array_type_by_catalog_name(key)
    }

    fn get_array_item_type(&self, _type_name: &TypeReference) -> Option<TypeReference> {
        todo!()
    }
}
