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
        AnalysisError, AnalysisProblem, CreateFunction, CreateIndex, CreateSequence, CreateTable,
        FullName, FunctionOverloadCollection, GetSourcePathWithSpanOpt, OperatorDefinition,
        OperatorName, SchemaName, TableName, TypeDefinition, TypeReference, ViewLike, ViewName,
        search_path::{
            expand_relation_search_path, expand_search_path_inst, expand_type_search_path,
        },
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

/// schema information database
///
/// `BaseParseContext` is a Immutable view of schema information,
///
/// [`crate::context::ParseContext`] presents mutate functions.
///
pub trait BaseParseContext:
    crate::sem::RoleInfoRead
    + crate::sem::OwnershipInfoRead
    + CastInfoRead
    + ConflictInfoRead
    + FunctionInfoRead
    + crate::sem::ExtensionInfoRead
    + crate::sem::ObjectInfoRead
    + IndexInfoRead
    + OperatorInfoRead
    + RuleInfoRead
    + TriggerInfoRead
    + SequenceInfoRead
    + TypeInfoRead
    + TableInfoRead
    + crate::sem::ViewInsertInfoRead
    + crate::sem::ViewUpdateInfoRead
    + crate::sem::ViewDeleteInfoRead
    + crate::sem::ViewMergeInfoRead
    + ViewInfoRead
    + SearchPathRead
    + GetSourcePathWithSpanOpt
    + std::fmt::Debug
{
    /// Resolve an exact SQLite relation key using ASCII case-insensitive identifiers.
    fn sqlite_relation_identity(&self, name: &FullName) -> Option<FullName>;

    /// Resolved metadata for the database selected by the caller.
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata;

    /// get context args
    fn get_args(&self) -> &SqlFunArgs;

    /// get SQL file path
    fn get_source_file_path(&self) -> Option<&Path>;

    /// get type by name
    fn get_type(&self, key: &FullName) -> Option<&TypeDefinition> {
        expand_type_search_path(self, key, |k| self.get_type_impl(k))
    }

    /// Look up an index, expanding unqualified names through the active search path.
    /// Explicitly qualified names are read directly.
    fn get_index(&self, name: &FullName) -> Option<&CreateIndex> {
        expand_relation_search_path(self, name, |key| self.get_index_impl(key))
    }

    /// Resolve a type to the exact qualified catalog key selected by the search path.
    fn resolve_type_name(&self, name: &FullName) -> Option<FullName> {
        expand_type_search_path(self, name, |key| {
            self.get_type_impl(key)?;
            Some(key.clone())
        })
    }

    /// get `CREATE TABLE` statement by table name
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        expand_relation_search_path(self, name.full_name(), |key| self.get_table_impl(key))
    }

    /// Resolve a table to the qualified catalog key that matched the search path.
    /// A stored declaration can itself have an unqualified name.
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName> {
        expand_relation_search_path(self, name.full_name(), |key| {
            self.get_table_impl(key)?;
            Some(TableName::from(key.clone()))
        })
    }

    /// Resolve a relation key in search-path order across both tables and views.
    fn resolve_relation_name(&self, name: &FullName) -> Option<FullName> {
        expand_relation_search_path(self, name, |key| relation_name_at(self, key))
    }

    /// get `CREATE VIEW` statement by view name
    fn get_view(&self, key: &ViewName) -> Option<&ViewLike> {
        expand_relation_search_path(self, key.full_name(), |key| self.get_view_impl(key))
    }

    /// get `CREATE SEQUENCE` statement by sequence name
    fn get_sequence(&self, seq_name: &FullName) -> Option<&CreateSequence> {
        expand_relation_search_path(self, seq_name, |key| self.get_sequence_impl(key))
    }

    /// get `CREATE FUNCTION` statement by function name
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        expand_search_path_inst(self, name, |key| self.get_function_impl(key))
    }

    /// get resolved function name
    fn resolve_function_name_with_search_path(&self, fun_name: &FullName) -> FullName {
        if fun_name.schema().is_some() {
            return fun_name.clone();
        }

        expand_search_path_inst(self, fun_name, |key| {
            self.get_function_impl(key).map(|_| key.clone())
        })
        .unwrap_or_else(|| fun_name.clone())
    }

    /// Canonicalize a reference to the catalog key selected by the search path.
    fn canonicalize_type_ref(&self, type_ref: &TypeReference) -> Option<TypeReference> {
        let full_name = self.resolve_type_name(type_ref.full_name())?;
        Some(TypeReference::concrete_type_ref(
            &full_name,
            type_ref.is_array().unwrap_or(false),
        ))
    }
}

#[cfg(test)]
mod canonicalization_tests;

/// Mutable diagnostic sink for semantic analysis.
pub trait DiagnosticContext {
    /// Report a recoverable analysis problem.
    ///
    /// Returns an error if the diagnostic cannot be accepted or rendered.
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError>;
}

fn relation_name_at(
    context: &(impl BaseParseContext + ?Sized),
    key: &FullName,
) -> Option<FullName> {
    if context.get_table(&TableName::from(key.clone())).is_some()
        || context.get_view(&ViewName::from_full_name(key)).is_some()
    {
        Some(key.clone())
    } else {
        None
    }
}
