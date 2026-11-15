use crate::{
    Collection,
    payloads::{AnalysisProbrem, CollectionReference, CollectionTraversal, ContextHandle},
};

/// Report current unmet lint expectations without ending or resetting analysis.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReportLintExpectationsArgs {
    /// Schema context whose accumulated observations are inspected.
    context_handle: ContextHandle,
}

impl ReportLintExpectationsArgs {
    /// Select an existing context; reporting does not restrict subsequent analysis.
    #[must_use]
    pub fn new(context_handle: &ContextHandle) -> Self {
        Self {
            context_handle: context_handle.clone(),
        }
    }
}

/// Immutable snapshot of unmet expectations as of this report call.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ReportLintExpectationsResponse {
    /// Unmet expectations, with no SQL reference spans.
    problems: Collection<AnalysisProbrem>,
}

impl ReportLintExpectationsResponse {
    /// Take ownership of the first bounded diagnostic page.
    #[must_use]
    pub const fn new(problems: Collection<AnalysisProbrem>) -> Self {
        Self { problems }
    }
}

impl CollectionTraversal for ReportLintExpectationsResponse {
    fn collections(&self) -> Vec<&dyn CollectionReference> {
        vec![&self.problems]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn CollectionReference> {
        vec![&mut self.problems]
    }
}
