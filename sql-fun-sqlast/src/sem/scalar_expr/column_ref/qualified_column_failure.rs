use crate::{
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, QueryProblem, SemScalarExpr,
        scalar_expr::column_ref::ColumnReferenceExpr,
    },
};

/// A completed failed lookup retaining the logical relation qualifier and column.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QualifiedColumnFailure {
    qualifier: FullName,
    column: ColumnName,
    kind: FailureKind,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum FailureKind {
    DatabaseNameRequired,
    CrossDatabase,
    InvalidQualification,
    SourceNotFound,
    SourceAmbiguous,
}

impl FailureKind {
    const fn database_required() -> Self {
        Self::DatabaseNameRequired
    }

    const fn cross_database() -> Self {
        Self::CrossDatabase
    }

    const fn invalid() -> Self {
        Self::InvalidQualification
    }

    const fn not_found() -> Self {
        Self::SourceNotFound
    }

    const fn ambiguous() -> Self {
        Self::SourceAmbiguous
    }

    const fn code(&self) -> &'static str {
        match self {
            Self::DatabaseNameRequired => "column_database_name_required",
            Self::CrossDatabase => "column_cross_database",
            Self::InvalidQualification => "column_qualification_invalid",
            Self::SourceNotFound => "column_source_not_found",
            Self::SourceAmbiguous => "column_source_ambiguous",
        }
    }

    fn problem(&self, reference: &str) -> QueryProblem {
        match self {
            Self::DatabaseNameRequired => QueryProblem::column_database_name_required(reference),
            Self::CrossDatabase => QueryProblem::column_cross_database(reference),
            Self::InvalidQualification => QueryProblem::column_qualification_invalid(reference),
            Self::SourceNotFound => QueryProblem::column_source_not_found(reference),
            Self::SourceAmbiguous => QueryProblem::column_source_ambiguous(reference),
        }
    }
}

impl QualifiedColumnFailure {
    /// Relation qualifier as logical components, including any requested database.
    pub const fn qualifier(&self) -> &FullName {
        &self.qualifier
    }
    /// Requested column as a logical identifier.
    pub const fn column(&self) -> &ColumnName {
        &self.column
    }
    /// Stable diagnostic code identifying the completed failure.
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    fn report(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
        kind: FailureKind,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let failure = Self {
            qualifier: qualifier.clone(),
            column: column.clone(),
            kind,
        };
        context.report_analysis_problem(AnalysisProblem::query(
            failure.kind.problem(&format!("{qualifier}.{column}")),
        ))?;
        Ok(SemScalarExpr::column_reference(
            ColumnReferenceExpr::qualified_failure(&failure),
        ))
    }

    pub(crate) fn database_required(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::report(context, qualifier, column, FailureKind::database_required())
    }
    pub(crate) fn cross_database(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::report(context, qualifier, column, FailureKind::cross_database())
    }
    pub(crate) fn invalid(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::report(context, qualifier, column, FailureKind::invalid())
    }
    pub(crate) fn not_found(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::report(context, qualifier, column, FailureKind::not_found())
    }
    pub(crate) fn ambiguous(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::report(context, qualifier, column, FailureKind::ambiguous())
    }
}
