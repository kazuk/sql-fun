use crate::sem::{AnalysisError, TypeReference};

/// A PostgreSQL GREATEST/LEAST type-resolution failure.
#[derive(Debug, Clone)]
pub enum MinMaxProblem {
    /// No operand was supplied.
    Empty,
    /// Two operand types cannot be coerced to the selected common type.
    Incompatible(Box<TypeReference>, Box<TypeReference>),
    /// Required type-category metadata is unavailable.
    MissingTypeFacts(TypeReference),
    /// Comparison metadata is unavailable for the result type.
    MissingComparisonFacts(TypeReference),
    /// The result type has no PostgreSQL comparison implementation.
    NotOrdered(TypeReference),
}
impl MinMaxProblem {
    pub(crate) const fn empty() -> Self {
        Self::Empty
    }
    pub(crate) fn incompatible(left: &TypeReference, right: &TypeReference) -> Self {
        Self::Incompatible(Box::new(left.clone()), Box::new(right.clone()))
    }
    pub(crate) fn missing_type_facts(ty: &TypeReference) -> Self {
        Self::MissingTypeFacts(ty.clone())
    }
    pub(crate) fn missing_comparison_facts(ty: &TypeReference) -> Self {
        Self::MissingComparisonFacts(ty.clone())
    }
    pub(crate) fn not_ordered(ty: &TypeReference) -> Self {
        Self::NotOrdered(ty.clone())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "min_max_empty",
            Self::Incompatible(..) => "min_max_incompatible_types",
            Self::MissingTypeFacts(_) => "min_max_missing_type_facts",
            Self::MissingComparisonFacts(_) => "min_max_missing_comparison_facts",
            Self::NotOrdered(_) => "min_max_not_ordered",
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &::miette::GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::Empty => "GREATEST/LEAST requires at least one argument".to_owned(),
            Self::Incompatible(left, right) => {
                format!("GREATEST/LEAST cannot match {left} and {right}")
            }
            Self::MissingTypeFacts(ty) => {
                format!("GREATEST/LEAST requires type-category metadata for {ty}")
            }
            Self::MissingComparisonFacts(ty) => {
                format!("GREATEST/LEAST requires comparison metadata for {ty}")
            }
            Self::NotOrdered(ty) => {
                format!("GREATEST/LEAST cannot identify a comparison function for {ty}")
            }
        };
        let diagnostic = ::miette::MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(::miette::Severity::Error);
        let report = ::miette::Report::new(diagnostic)
            .with_source_code(::miette::NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}
