//! DB Engine specific syntax hooks

#[cfg(feature = "sqlite")]
use crate::sem::AnalysisError;

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{Array, Subscript};

/// array feature in turso
#[cfg(feature = "sqlite")]
pub trait TursoArrayFeature {
    /// notify [`crate::syn::sqlite::Array`] used
    fn array_element_access(&mut self, array: &Array) -> Result<(), AnalysisError>;

    /// notify [`crate::syn::sqlite::Subscript`] base used
    fn subscript_base_access(&mut self, subscript: &Subscript) -> Result<(), AnalysisError>;

    /// notify [`crate::syn::sqlite::Subscript`] index used
    fn subscript_index_access(&mut self, subscript: &Subscript) -> Result<(), AnalysisError>;
}
