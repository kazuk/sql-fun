use crate::sem::{
    AnalysisError, AnalysisProblem, FrozenBaseContext, FullName, SemAst, TypeDefinition,
    TypeInfoRead, TypeReference,
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
}

impl ParsedSchemaDump {
    pub(crate) const fn new(
        sem_results: Vec<SemAst>,
        base: FrozenBaseContext,
        problems: Vec<AnalysisProblem>,
    ) -> Self {
        Self {
            _sem_results: sem_results,
            base,
            problems,
        }
    }

    /// The completed catalog, suitable as the explicit base for another file.
    ///
    /// Cloning this handle keeps the catalog alive independently of this parse
    /// result, without retaining its semantic statements or problems.
    pub const fn base_context(&self) -> &FrozenBaseContext {
        &self.base
    }

    /// Recoverable problems from this file, in reporting order.
    ///
    /// Problems from earlier files stay on their own parse results.
    pub fn problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }
}

impl TypeInfoRead for ParsedSchemaDump {
    fn domain_base_type(&self, name: &FullName) -> Option<TypeReference> {
        self.base_context().domain_base_type(name)
    }

    fn builtin_type_category(&self, name: &FullName) -> Option<(char, bool)> {
        self.base_context().builtin_type_category(name)
    }
    fn builtin_type_ordered(&self, name: &FullName) -> bool {
        self.base_context().builtin_type_ordered(name)
    }

    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.base_context().get_type_impl(key)
    }

    fn get_array_item_type(&self, _type_name: &TypeReference) -> Option<TypeReference> {
        todo!()
    }
}
