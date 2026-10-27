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

/// Turso sequence syntax notifications after syntax recognition.
#[cfg(feature = "sqlite")]
pub trait TursoSequenceFeature {
    /// Notify that a sequence is created.
    fn create_sequence_used(
        &mut self,
        statement: &crate::syn::sqlite::CreateSequenceStatement,
    ) -> Result<(), AnalysisError>;
    /// Notify that a sequence is dropped.
    fn drop_sequence_used(
        &mut self,
        statement: &crate::syn::sqlite::DropSequenceStatement,
    ) -> Result<(), AnalysisError>;
}

/// Turso ordered-set aggregate syntax notification after syntax recognition.
#[cfg(feature = "sqlite")]
pub trait TursoOrderedSetAggregateFeature {
    /// Notify that a function uses WITHIN GROUP ordering.
    fn ordered_set_aggregate_used(
        &mut self,
        expression: &crate::syn::sqlite::ExprFunctionCall,
    ) -> Result<(), AnalysisError>;
}

/// Turso MVCC garbage-collection pragma notification after syntax recognition.
#[cfg(feature = "sqlite")]
pub trait TursoMvccGcFeature {
    /// Notify that the MVCC garbage-collection threshold pragma is used.
    fn mvcc_gc_threshold_used(
        &mut self,
        pragma: &crate::syn::sqlite::PragmaStatement,
    ) -> Result<(), AnalysisError>;
}

/// Turso JSON EXPLAIN QUERY PLAN syntax notification.
#[cfg(feature = "sqlite")]
pub trait TursoEqpFeature {
    /// Notify that JSON query-plan output is requested.
    fn json_eqp_used(
        &mut self,
        command: &crate::syn::sqlite::ExplainQueryPlan,
    ) -> Result<(), AnalysisError>;
}

/// Turso MVCC group-commit pragma notification.
#[cfg(feature = "sqlite")]
pub trait TursoMvccGroupCommitFeature {
    /// Notify that the MVCC group-commit pragma is used.
    fn mvcc_group_commit_used(
        &mut self,
        pragma: &crate::syn::sqlite::PragmaStatement,
    ) -> Result<(), AnalysisError>;
}

/// Turso FTS merge-threshold pragma notification.
#[cfg(feature = "sqlite")]
pub trait TursoFtsMergeThresholdFeature {
    /// Notify that the FTS merge-threshold pragma is used.
    fn fts_merge_threshold_used(
        &mut self,
        pragma: &crate::syn::sqlite::PragmaStatement,
    ) -> Result<(), AnalysisError>;
}

/// Turso array type syntax notification.
#[cfg(feature = "sqlite")]
pub trait TursoArrayTypeFeature {
    /// Notify that a type has parser-provided array dimensions.
    fn array_type_used(&mut self, ty: &crate::syn::sqlite::Type) -> Result<(), AnalysisError>;
}
