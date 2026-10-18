use crate::{
    StringSpan,
    sem::{AnalysisError, FullName, Kind, OwnershipIdentity},
};

/// A rejected creation, retaining namespace and qualified semantic identity.
#[derive(Clone, Debug)]
pub struct CreationProblem {
    name: FullName,
    namespace: &'static str,
    requested: Option<Kind>,
    existing: Option<Kind>,
    signature: Option<OwnershipIdentity>,
    container: Option<FullName>,
    span: StringSpan,
}
impl CreationProblem {
    pub(crate) fn named(name: &FullName, namespace: &'static str, span: &StringSpan) -> Self {
        Self {
            name: name.clone(),
            namespace,
            requested: None,
            existing: None,
            signature: None,
            container: None,
            span: *span,
        }
    }
    pub(crate) fn relation(
        name: &FullName,
        requested: Kind,
        existing: Kind,
        span: &StringSpan,
    ) -> Self {
        let mut problem = Self::named(name, "relation", span);
        problem.requested = Some(requested);
        problem.existing = Some(existing);
        problem
    }
    pub(crate) fn constraint(table: &FullName, name: &str, span: &StringSpan) -> Self {
        let mut problem = Self::named(&FullName::from_local_name(name), "constraint", span);
        problem.container = Some(table.clone());
        problem
    }
    /// Relation containing the conflicting constraint, when applicable.
    pub const fn container(&self) -> Option<&FullName> {
        self.container.as_ref()
    }
    pub(crate) fn routine(identity: &OwnershipIdentity, span: &StringSpan) -> Self {
        let mut problem = Self::named(identity.name(), "routine", span);
        problem.signature = Some(identity.clone());
        problem
    }
    /// Effective qualified name that could not be created.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// Namespace or creation policy that rejected the definition.
    pub const fn namespace(&self) -> &'static str {
        self.namespace
    }
    /// Requested relation kind, when the conflict is in the relation namespace.
    pub const fn requested_kind(&self) -> Option<Kind> {
        self.requested
    }
    /// Existing relation kind, when the conflict is in the relation namespace.
    pub const fn existing_kind(&self) -> Option<Kind> {
        self.existing
    }
    /// Routine signature, when the conflict concerns an overload.
    pub const fn signature(&self) -> Option<&OwnershipIdentity> {
        self.signature.as_ref()
    }
    pub(super) fn render_report(
        &self,
        handler: &::miette::GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = if let (Some(requested), Some(existing)) = (self.requested, self.existing) {
            format!(
                "cannot create {requested} {}: name is occupied by {existing}",
                self.name
            )
        } else {
            format!("conflicting {} definition: {}", self.namespace, self.name)
        };
        let diagnostic = ::miette::MietteDiagnostic::new(message)
            .with_code("conflicting_creation")
            .with_severity(::miette::Severity::Error)
            .with_label(
                StringSpan::new(0, self.span.len()).new_labeled_span("conflicting definition"),
            );
        let report = ::miette::Report::new(diagnostic)
            .with_source_code(::miette::NamedSource::new("SQL", source));
        let mut output = String::new();
        handler.render_report(&mut output, &*report)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::CreationProblem;
    use crate::{
        StringSpan,
        sem::{FullName, Kind},
    };

    #[test]
    fn renders_against_the_statement_source_slice() -> testresult::TestResult {
        let sql = "CREATE TABLE public.t(id integer)";
        let problem = CreationProblem::relation(
            &FullName::with_schema("public", "t"),
            Kind::table(),
            Kind::view(),
            &StringSpan::new(120, sql.len()),
        );
        let report =
            problem.render_report(&::miette::GraphicalReportHandler::new(), sql.to_owned())?;
        assert!(report.contains("conflicting_creation"), "{report}");
        assert!(report.contains("occupied by view"), "{report}");
        assert!(report.contains(sql), "{report}");
        Ok(())
    }
}
