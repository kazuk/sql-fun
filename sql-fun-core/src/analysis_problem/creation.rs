use crate::analysis_problem::{DiagnosticMetadata, FullName, Kind, OwnershipIdentity, StringSpan};

/// A rejected creation, retaining namespace and qualified semantic identity.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct CreationProblem {
    name: FullName,
    namespace: String,
    requested: Option<Kind>,
    existing: Option<Kind>,
    signature: Option<OwnershipIdentity>,
    container: Option<FullName>,
    span: StringSpan,
}
impl CreationProblem {
    /// Diagnostic named.
    #[must_use]
    pub fn named(name: impl Into<FullName>, namespace: &str, span: impl Into<StringSpan>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.to_owned(),
            requested: None,
            existing: None,
            signature: None,
            container: None,
            span: span.into(),
        }
    }
    /// Diagnostic relation.
    #[must_use]
    pub fn relation(
        name: impl Into<FullName>,
        requested: impl Into<Kind>,
        existing: impl Into<Kind>,
        span: impl Into<StringSpan>,
    ) -> Self {
        let mut problem = Self::named(name, "relation", span);
        problem.requested = Some(requested.into());
        problem.existing = Some(existing.into());
        problem
    }
    /// Diagnostic constraint.
    #[must_use]
    pub fn constraint(
        table: impl Into<FullName>,
        name: impl Into<FullName>,
        span: impl Into<StringSpan>,
    ) -> Self {
        let mut problem = Self::named(name, "constraint", span);
        problem.container = Some(table.into());
        problem
    }
    /// Relation containing the conflicting constraint, when applicable.
    #[must_use]
    pub const fn container(&self) -> Option<&FullName> {
        self.container.as_ref()
    }
    /// Diagnostic routine.
    #[must_use]
    pub fn routine(
        name: impl Into<FullName>,
        identity: impl Into<OwnershipIdentity>,
        span: impl Into<StringSpan>,
    ) -> Self {
        let identity = identity.into();
        let mut problem = Self::named(name, "routine", span);
        problem.signature = Some(identity);
        problem
    }
    /// Effective qualified name that could not be created.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// Namespace or creation policy that rejected the definition.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    /// Requested relation kind, when the conflict is in the relation namespace.
    #[must_use]
    pub const fn requested_kind(&self) -> Option<&Kind> {
        self.requested.as_ref()
    }
    /// Existing relation kind, when the conflict is in the relation namespace.
    #[must_use]
    pub const fn existing_kind(&self) -> Option<&Kind> {
        self.existing.as_ref()
    }
    /// Routine signature, when the conflict concerns an overload.
    #[must_use]
    pub const fn signature(&self) -> Option<&OwnershipIdentity> {
        self.signature.as_ref()
    }
    /// Diagnostic presentation data.
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        let message = if let (Some(requested), Some(existing)) = (&self.requested, &self.existing) {
            format!(
                "cannot create {requested} {}: name is occupied by {existing}",
                self.name
            )
        } else {
            format!("conflicting {} definition: {}", self.namespace, self.name)
        };
        DiagnosticMetadata::from(message).with_label(
            "conflicting definition",
            StringSpan::new(0, self.span.len()),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis_problem::creation::{CreationProblem, FullName, Kind, StringSpan};

    #[test]
    fn labels_the_statement_source_slice() {
        let sql = "CREATE TABLE public.t(id integer)";
        let problem = CreationProblem::relation(
            FullName::new("public.t"),
            Kind::new("table"),
            Kind::new("view"),
            StringSpan::new(120, sql.len()),
        );
        let metadata = problem.render_metadata();
        assert!(metadata.message().contains("occupied by view"));
        assert_eq!(metadata.labels()[0].span(), StringSpan::new(0, sql.len()));
        assert_eq!(metadata.labels()[0].message(), "conflicting definition");
    }
}
