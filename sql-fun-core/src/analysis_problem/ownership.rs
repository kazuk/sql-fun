use crate::analysis_problem::{DiagnosticMetadata, OwnershipLookup};

/// Structured ownership lookup failure, preserving semantic identities.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct OwnershipProblem {
    target: Option<OwnershipLookup>,
    role: Option<String>,
}
impl OwnershipProblem {
    /// Diagnostic target.
    #[must_use]
    pub fn target(target: impl Into<OwnershipLookup>) -> Self {
        Self {
            target: Some(target.into()),
            role: None,
        }
    }
    /// Diagnostic missing role.
    #[must_use]
    pub fn missing_role(name: &str) -> Self {
        Self {
            target: None,
            role: Some(name.to_owned()),
        }
    }
    /// Failed target lookup, when the problem concerns the object.
    #[must_use]
    pub const fn target_outcome(&self) -> Option<&OwnershipLookup> {
        self.target.as_ref()
    }
    /// Exact missing role name, when the problem concerns the owner.
    #[must_use]
    pub fn role_name(&self) -> Option<&str> {
        self.role.as_deref()
    }
    /// Diagnostic code.
    pub(super) fn code(&self) -> &str {
        if let Some(target) = &self.target {
            target.code()
        } else {
            "ownership_role_not_found"
        }
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = if let Some(role) = &self.role {
            format!("owner role does not exist: {role}")
        } else {
            self.target.as_ref().map_or_else(
                || "ownership lookup failed".to_owned(),
                OwnershipLookup::diagnostic_message,
            )
        };
        DiagnosticMetadata::from(message)
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis_problem::ownership::{OwnershipLookup, OwnershipProblem};

    #[rstest::rstest]
    #[case::role(
        OwnershipProblem::missing_role("missing_owner"),
        "ownership_role_not_found",
        "owner role does not exist: missing_owner"
    )]
    #[case::target(
        OwnershipProblem::target(OwnershipLookup::new(
            "ownership_target_not_found",
            "missing schema"
        )),
        "ownership_target_not_found",
        "missing schema"
    )]
    fn preserves_outcome_classification_and_message(
        #[case] problem: OwnershipProblem,
        #[case] code: &str,
        #[case] message: &str,
    ) {
        assert_eq!(problem.code(), code);
        assert_eq!(problem.render_metadata().message(), message);
    }
}
