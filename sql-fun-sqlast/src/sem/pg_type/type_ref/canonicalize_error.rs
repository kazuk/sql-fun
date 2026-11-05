use crate::sem::{AnalysisError, TypeReference};
impl TypeReference {
    /// Inconsistent type reference.
    #[track_caller]
    #[must_use]
    pub fn canonicalize_error(&self) -> AnalysisError {
        todo!()
    }
}
