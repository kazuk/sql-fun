use crate::sem::{AnalysisError, FullName, TriggerIdentity};
/// Structured trigger-definition validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerProblem {
    /// Missing or invalid target relation.
    Target(FullName),
    /// Duplicate trigger in its dialect namespace.
    Duplicate(TriggerIdentity),
    /// Requested trigger does not exist in the resolved relation namespace.
    Missing(TriggerIdentity),
    /// No SQLite trigger matched an unbound requested name.
    MissingName(FullName),
    /// Unknown event column.
    Column(FullName, String),
    /// Missing or invalid zero-input trigger function.
    Function(FullName),
    /// Incompatible trigger options.
    Options(FullName, String),
}
impl TriggerProblem {
    pub(crate) fn target(name: &FullName) -> Self {
        Self::Target(name.clone())
    }
    pub(crate) fn duplicate(identity: &TriggerIdentity) -> Self {
        Self::Duplicate(identity.clone())
    }
    pub(crate) fn missing_name(name: &FullName) -> Self {
        Self::MissingName(name.clone())
    }
    pub(crate) fn missing(identity: &TriggerIdentity) -> Self {
        Self::Missing(identity.clone())
    }
    pub(crate) fn column(target: &FullName, column: &str) -> Self {
        Self::Column(target.clone(), column.to_owned())
    }
    pub(crate) fn function(name: &FullName) -> Self {
        Self::Function(name.clone())
    }
    pub(crate) fn options(target: &FullName, rule: &str) -> Self {
        Self::Options(target.clone(), rule.to_owned())
    }
    pub(super) fn render_report(
        &self,
        handler: &::miette::GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let diagnostic =
            ::miette::MietteDiagnostic::new(format!("invalid trigger statement: {self:?}"))
                .with_code("trigger_invalid_definition")
                .with_severity(::miette::Severity::Error);
        let report = ::miette::Report::new(diagnostic)
            .with_source_code(::miette::NamedSource::new("SQL", source));
        let mut rendered = String::new();
        handler.render_report(&mut rendered, &*report)?;
        Ok(rendered)
    }
}
