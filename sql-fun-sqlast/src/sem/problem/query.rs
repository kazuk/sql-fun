use crate::sem::AnalysisError;
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured violations in SELECT clauses, distinct from retained name bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryProblem {
    /// A WINDOW declaration repeats an existing definition name.
    DuplicateWindowDefinition(String),
    /// A clause name has no applicable definition.
    ReferenceNotFound(String),
    /// A clause name identifies more than one applicable definition.
    AmbiguousReference(String),
    /// An ORDER/GROUP BY ordinal is outside the output list.
    PositionOutOfRange(i64),
    /// A compound ORDER BY expression does not identify an output column.
    InvalidCompoundOrder,
}

impl QueryProblem {
    pub(crate) fn duplicate_window_definition(name: &str) -> Self {
        Self::DuplicateWindowDefinition(name.to_owned())
    }
    pub(crate) fn reference_not_found(name: &str) -> Self {
        Self::ReferenceNotFound(name.to_owned())
    }
    pub(crate) fn ambiguous_reference(name: &str) -> Self {
        Self::AmbiguousReference(name.to_owned())
    }
    pub(crate) const fn position_out_of_range(position: i64) -> Self {
        Self::PositionOutOfRange(position)
    }
    pub(crate) const fn invalid_compound_order() -> Self {
        Self::InvalidCompoundOrder
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::DuplicateWindowDefinition(_) => "query_duplicate_window_definition",
            Self::ReferenceNotFound(_) => "query_reference_not_found",
            Self::AmbiguousReference(_) => "query_reference_ambiguous",
            Self::PositionOutOfRange(_) => "query_position_out_of_range",
            Self::InvalidCompoundOrder => "query_invalid_compound_order",
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = match self {
            Self::DuplicateWindowDefinition(name) => format!("duplicate window definition: {name}"),
            Self::ReferenceNotFound(name) => format!("clause reference not found: {name}"),
            Self::AmbiguousReference(name) => format!("ambiguous clause reference: {name}"),
            Self::PositionOutOfRange(position) => {
                format!("ORDER/GROUP BY position {position} is outside the select list")
            }
            Self::InvalidCompoundOrder => {
                "compound ORDER BY must refer to a result column".to_owned()
            }
        };
        let diagnostic = MietteDiagnostic::new(message)
            .with_code(self.code())
            .with_severity(Severity::Error);
        let report = Report::new(diagnostic).with_source_code(NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::QueryProblem;

    #[rstest::rstest]
    #[case::duplicate(
        QueryProblem::duplicate_window_definition("w"),
        "duplicate window definition: w"
    )]
    #[case::missing(
        QueryProblem::reference_not_found("w"),
        "clause reference not found: w"
    )]
    #[case::ambiguous(
        QueryProblem::ambiguous_reference("w"),
        "ambiguous clause reference: w"
    )]
    #[case::position(
        QueryProblem::position_out_of_range(3),
        "position 3 is outside the select list"
    )]
    #[case::compound(
        QueryProblem::invalid_compound_order(),
        "compound ORDER BY must refer to a result column"
    )]
    fn renders_structured_problem(#[case] problem: QueryProblem, #[case] message: &str) {
        let report = problem
            .render_report(
                &::miette::GraphicalReportHandler::new(),
                "SELECT 1".to_owned(),
            )
            .unwrap();
        assert!(report.contains(message));
        assert!(report.contains(problem.code()));
    }
}
