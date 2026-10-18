use crate::sem::{AnalysisError, OwnershipLookup};
use ::miette::{GraphicalReportHandler, MietteDiagnostic, NamedSource, Report, Severity};

/// Structured ownership lookup failure, preserving semantic identities.
#[derive(Clone, Debug)]
pub struct OwnershipProblem {
    target: Option<OwnershipLookup>,
    role: Option<String>,
}
impl OwnershipProblem {
    pub(crate) fn target(target: &OwnershipLookup) -> Self {
        Self {
            target: Some(target.clone()),
            role: None,
        }
    }
    pub(crate) fn missing_role(name: &str) -> Self {
        Self {
            target: None,
            role: Some(name.to_owned()),
        }
    }
    /// Failed target lookup, when the problem concerns the object.
    pub const fn target_outcome(&self) -> Option<&OwnershipLookup> {
        self.target.as_ref()
    }
    /// Exact missing role name, when the problem concerns the owner.
    pub fn role_name(&self) -> Option<&str> {
        self.role.as_deref()
    }
    pub(crate) const fn code(&self) -> &'static str {
        if let Some(target) = &self.target {
            target.code()
        } else {
            "ownership_role_not_found"
        }
    }
    pub(super) fn render_report(
        &self,
        handler: &GraphicalReportHandler,
        source: String,
    ) -> Result<String, AnalysisError> {
        let message = if let Some(role) = &self.role {
            format!("owner role does not exist: {role}")
        } else {
            self.target.as_ref().map_or_else(
                || "ownership lookup failed".to_owned(),
                OwnershipLookup::diagnostic_message,
            )
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
    use super::OwnershipProblem;
    use crate::sem::{FullName, ObjectKind, OwnershipIdentity, OwnershipLookup};

    #[rstest::rstest]
    #[case::role(
        OwnershipProblem::missing_role("missing_owner"),
        "ownership_role_not_found",
        "missing_owner"
    )]
    #[case::target(OwnershipProblem::target(&OwnershipLookup::not_found(&OwnershipIdentity::named(ObjectKind::from(37), &FullName::from_local_name("missing_schema")))), "ownership_target_not_found", "missing_schema")]
    fn diagnostic_reports_preserve_the_failure_reason(
        #[case] problem: OwnershipProblem,
        #[case] code: &str,
        #[case] name: &str,
    ) -> testresult::TestResult {
        let report = problem.render_report(
            &::miette::GraphicalReportHandler::new(),
            "ALTER SCHEMA missing_schema OWNER TO missing_owner".to_owned(),
        )?;
        assert!(report.contains(code), "{report}");
        assert!(report.contains(name), "{report}");
        Ok(())
    }
}
