//! DB Engine specific syntax hooks

#[cfg(feature = "sqlite")]
use crate::syn::sqlite::CreateSequenceStatement;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::DropSequenceStatement;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExplainQueryPlan;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ExprFunctionCall;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::PragmaStatement;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Type;
use crate::{ast::Dialect, context::ParseContext, sem::AnalysisProblem};

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
        statement: &CreateSequenceStatement,
    ) -> Result<(), AnalysisError>;
    /// Notify that a sequence is dropped.
    fn drop_sequence_used(
        &mut self,
        statement: &DropSequenceStatement,
    ) -> Result<(), AnalysisError>;
}

/// Turso ordered-set aggregate syntax notification after syntax recognition.
#[cfg(feature = "sqlite")]
pub trait TursoOrderedSetAggregateFeature {
    /// Notify that a function uses WITHIN GROUP ordering.
    fn ordered_set_aggregate_used(
        &mut self,
        expression: &ExprFunctionCall,
    ) -> Result<(), AnalysisError>;
}

/// Turso MVCC garbage-collection pragma notification after syntax recognition.
#[cfg(feature = "sqlite")]
pub trait TursoMvccGcFeature {
    /// Notify that the MVCC garbage-collection threshold pragma is used.
    fn mvcc_gc_threshold_used(&mut self, pragma: &PragmaStatement) -> Result<(), AnalysisError>;
}

/// Turso JSON EXPLAIN QUERY PLAN syntax notification.
#[cfg(feature = "sqlite")]
pub trait TursoEqpFeature {
    /// Notify that JSON query-plan output is requested.
    fn json_eqp_used(&mut self, command: &ExplainQueryPlan) -> Result<(), AnalysisError>;
}

/// Turso MVCC group-commit pragma notification.
#[cfg(feature = "sqlite")]
pub trait TursoMvccGroupCommitFeature {
    /// Notify that the MVCC group-commit pragma is used.
    fn mvcc_group_commit_used(&mut self, pragma: &PragmaStatement) -> Result<(), AnalysisError>;
}

/// Turso FTS merge-threshold pragma notification.
#[cfg(feature = "sqlite")]
pub trait TursoFtsMergeThresholdFeature {
    /// Notify that the FTS merge-threshold pragma is used.
    fn fts_merge_threshold_used(&mut self, pragma: &PragmaStatement) -> Result<(), AnalysisError>;
}

/// Turso array type syntax notification.
#[cfg(feature = "sqlite")]
pub trait TursoArrayTypeFeature {
    /// Notify that a type has parser-provided array dimensions.
    fn array_type_used(&mut self, ty: &Type) -> Result<(), AnalysisError>;
}

/// PostgreSQL 18 syntax recognized independently of the selected target engine.
#[derive(Clone, Copy, Debug)]
pub enum Postgres18Syntax {
    /// Default privileges on large objects.
    LargeObjectDefaultPrivileges,
    /// LIKE elements in a foreign table definition.
    ForeignTableLike,
    /// Explicit OLD/NEW aliases in RETURNING WITH.
    ReturningAliases,
    /// Special OLD/NEW destination row images in RETURNING.
    ReturningRowImages,
    /// Virtual generated column, including the omitted storage kind.
    VirtualGeneratedColumn,
    /// Temporal primary/unique or foreign-key constraint.
    TemporalConstraint,
    /// Explicit constraint enforcement attributes.
    ConstraintEnforcement,
    /// Table-level NOT NULL and new nullability attributes.
    NotNullConstraint,
    /// ALTER CONSTRAINT INHERIT / NO INHERIT.
    ConstraintInheritance,
}

impl Postgres18Syntax {
    /// SQL construct named by the version diagnostic.
    pub const fn description(self) -> &'static str {
        match self {
            Self::LargeObjectDefaultPrivileges => "default privileges on LARGE OBJECTS",
            Self::ForeignTableLike => "CREATE FOREIGN TABLE LIKE",
            Self::ReturningAliases => "RETURNING WITH row aliases",
            Self::ReturningRowImages => "RETURNING OLD/NEW row images",
            Self::VirtualGeneratedColumn => "virtual generated columns",
            Self::TemporalConstraint => "temporal constraints",
            Self::ConstraintEnforcement => "constraint ENFORCED/NOT ENFORCED",
            Self::NotNullConstraint => "PostgreSQL 18 NOT NULL constraints",
            Self::ConstraintInheritance => "ALTER CONSTRAINT inheritance",
        }
    }
}

/// Feature notification after PostgreSQL syntax recognition.
pub trait Postgres18Feature {
    /// Report incompatible target versions; return whether analysis can proceed.
    fn postgres18_syntax_used(&mut self, syntax: Postgres18Syntax) -> Result<bool, AnalysisError>;
}

impl<T: ParseContext> Postgres18Feature for T {
    fn postgres18_syntax_used(&mut self, syntax: Postgres18Syntax) -> Result<bool, AnalysisError> {
        let selected = self
            .metadata()
            .engine_version()
            .as_ref()
            .map(ToString::to_string);
        let compatible = self.dialect() == Dialect::postgres()
            && selected.as_deref().is_some_and(|version| {
                version
                    .parse::<::sql_fun_core::PostgresVersion>()
                    .is_ok_and(|version| version.major() >= 18)
            });
        if !compatible {
            self.report_analysis_problem(AnalysisProblem::incompatible_engine_version(
                syntax.description(),
                18,
                selected.as_deref(),
            ))?;
        }
        Ok(compatible)
    }
}
