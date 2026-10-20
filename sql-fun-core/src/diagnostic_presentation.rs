use crate::{SqlDialect, analysis_problem::AnalysisProblem};

/// An analyzed SQL statement and its diagnostic, supplied for presentation.
///
/// Diagnostic label spans are relative to `source`, while `statement_offset`
/// records the statement's byte offset in the explicitly analyzed file.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticPresentation {
    problem: AnalysisProblem,
    source: String,
    dialect: SqlDialect,
    statement_offset: usize,
}

impl DiagnosticPresentation {
    /// Retain the exact analyzed statement without reopening its source file.
    #[must_use]
    pub fn new(
        problem: &AnalysisProblem,
        source: &str,
        dialect: SqlDialect,
        statement_offset: usize,
    ) -> Self {
        Self {
            problem: problem.clone(),
            source: source.to_owned(),
            dialect,
            statement_offset,
        }
    }

    /// Structured semantic diagnostic, independent of terminal rendering.
    #[must_use]
    pub const fn problem(&self) -> &AnalysisProblem {
        &self.problem
    }

    /// Exact statement text used during analysis.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Explicit dialect used to analyze this statement.
    #[must_use]
    pub const fn dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// Statement's byte offset within its source file.
    #[must_use]
    pub const fn statement_offset(&self) -> usize {
        self.statement_offset
    }
}
