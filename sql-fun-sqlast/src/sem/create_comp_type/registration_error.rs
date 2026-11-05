use crate::sem::{AnalysisError, CreateCompositeType};
use std::convert::Infallible;
impl CreateCompositeType {
    /// Reject registration of an anonymous composite in the catalog.
    pub fn raise_registration_error(&self) -> Result<Infallible, AnalysisError> {
        AnalysisError::raise_unexpected_input(
            "anonymous composite cannot be registered in the catalog",
        )
    }
}
