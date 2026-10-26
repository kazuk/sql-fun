mod cast_info;
pub(crate) mod conflict_info;
mod cursors;
mod rule_info;
mod seq_info;
mod table_info;
mod trigger_info;
pub(crate) mod type_hint;
mod type_info;
mod view_info;

use std::{
    io::{Read, Seek},
    path::Path,
};

pub use self::{
    cast_info::CastInfoRead,
    conflict_info::{CatalogNameLookup, ConflictArbiterInference, ConflictInfoRead},
    cursors::CursorInfo,
    rule_info::{RuleInfoRead, RuleInfoWrite},
    seq_info::{SequenceInfoRead, SequenceInfoWrite},
    table_info::{TableInfoRead, TableInfoWrite},
    trigger_info::{TriggerInfoRead, TriggerInfoWrite},
    type_hint::{ParameterTypeHintRead, ParameterTypeHintWrite},
    type_info::{TypeInfoRead, TypeInfoWrite},
    view_info::{ViewInfoRead, ViewInfoWrite},
};

pub(crate) use self::cursors::CursorBindings;

use crate::{
    StringSpan,
    sem::{
        AnalysisError, AnalysisProblem, CreateFunction, CreateIndex, FullName,
        FunctionOverloadCollection, OperatorDefinition, OperatorName, SchemaName, TableName,
    },
};
use sql_fun_core::SqlFunArgs;

/// provide access to source SQL file
pub trait SourceAccessService: std::fmt::Debug {
    /// Get source span as String
    fn get_source_span(
        &self,
        _config_args: &SqlFunArgs,
        source_path: &Path,
        span: StringSpan,
    ) -> Result<String, AnalysisError> {
        let source_path = if source_path.is_absolute() {
            source_path
        } else {
            // TODO resolve path
            source_path
        };

        if !source_path.exists() {
            AnalysisError::file_not_found(source_path)?;
            AnalysisError::never()
        };

        let mut file = std::fs::File::open(source_path)?;
        file.seek(span.seek_pos())?;
        let mut source_span = file.take(span.len() as u64);
        let mut buf = String::default();
        let result_len = source_span.read_to_string(&mut buf)?;
        if result_len != span.len() {
            Err(AnalysisError::unexpected_eof())
        } else {
            Ok(buf)
        }
    }
}

/// default implementation for SourceAccessService
#[derive(Debug, Default)]
pub struct DefaultSourceAccessService {}

impl SourceAccessService for DefaultSourceAccessService {}

/// get index definition
pub trait IndexInfoRead {
    /// Enumerate indexes belonging to the qualified destination table.
    fn indexes_for_table(&self, table: &TableName) -> Vec<&CreateIndex>;

    /// Read an index by its exact qualified identity, without search-path expansion.
    fn get_index_impl(&self, index_name: &FullName) -> Option<&CreateIndex>;
}

/// get function definition
pub trait FunctionInfoRead {
    /// Declared routine identities that can depend on schema-defined row types.
    /// Immutable builtin-only catalogs have no such declarations.
    fn declared_routine_names(&self) -> Vec<FullName> {
        Vec::new()
    }

    /// Whether an exactly qualified routine is an aggregate. Ordinary routines
    /// are the default; catalogs retaining aggregates must override this method.
    /// Callers must first resolve the routine through the selected catalog.
    fn is_aggregate_impl(&self, _name: &FullName) -> bool {
        false
    }
    /// Whether this exact function overload is scalar and preserves source rows
    /// in a view target list. Aggregate/window/set-returning calls return false.
    /// A provider lacking function-kind metadata must remain explicitly unimplemented.
    fn function_preserves_view_rows(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool;

    /// get `CREATE FUNCTION` statement by function name
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection>;
}

/// store and update function info
pub trait FunctionInfoWrite
where
    Self: Sized,
{
    /// Record source function properties separately from its readable signature.
    fn apply_function_view_rows(
        self,
        function: &CreateFunction,
        preserves_rows: bool,
    ) -> Result<Self, AnalysisError>;

    /// apply `create function` statement (implementation)
    fn apply_create_function_impl(
        self,
        name: &FullName,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError>;
}

/// get operator definition
pub trait OperatorInfoRead {
    /// A catalog guarantee that the operator returns a non-null value when
    /// both operands are non-null. Strictness alone does not provide this.
    /// `None` means no guarantee is available, including custom implementations.
    fn operator_result_is_not_null(&self, _name: &OperatorName) -> Option<bool> {
        None
    }

    /// get operator definition,
    ///
    /// `has_left`, `has_right` both true then returns Binary operator.
    /// one of `has_left`, `has_right` is true then returns Left unary or Right unary operator.
    ///
    fn get_operator_op(
        &self,
        name: &OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&OperatorDefinition>;
}

/// search path
pub trait SearchPathRead {
    /// get search path
    fn get_search_path(&self) -> &[SchemaName];

    /// gets default schema.
    ///
    /// # Note
    ///
    /// returns first item of current search path
    fn get_default_schema(&self) -> Option<SchemaName>;
}

/// Mutable diagnostic sink for semantic analysis.
pub trait DiagnosticContext {
    /// Report a recoverable analysis problem.
    ///
    /// Returns an error if the diagnostic cannot be accepted or rendered.
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError>;
}
