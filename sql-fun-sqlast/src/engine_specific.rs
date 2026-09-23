//! DB Engine specific syntax hooks

#[cfg(feature = "sqlite")]
use crate::sem::AnalysisError;

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{Array, CreateMaterializedView, OptimizeStatement, Subscript};
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::{CreateTypeStatement, DropTypeStatement};

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

/// materialized view
#[cfg(feature = "sqlite")]
pub trait TursoMaterializedViewFeature {
    /// notify [`crate::syn::sqlite::CreateMaterializedView`] used
    fn create_materialized_view_used(
        &mut self,
        mat_view: &CreateMaterializedView,
    ) -> Result<(), AnalysisError>;
}

/// custom type
#[cfg(feature = "sqlite")]
pub trait TursoCustomTypeFeature {
    /// notify [`crate::syn::sqlite::CreateTypeStatement`] used
    fn create_type_used(&mut self, create_type: &CreateTypeStatement) -> Result<(), AnalysisError>;

    /// notify [`crate::syn::sqlite::DropTypeStatement`] used
    fn drop_type_used(&mut self, drop_type: &DropTypeStatement) -> Result<(), AnalysisError>;
}

/// `OPTIMIZE INDEX` statement used
#[cfg(feature = "sqlite")]
pub trait TursoOptimizeIndexFeature {
    /// notify [`crate::syn::sqlite::OptimizeStatement`] used
    fn optimize_statement_used(
        &mut self,
        optimize: &OptimizeStatement,
    ) -> Result<(), AnalysisError>;
}
