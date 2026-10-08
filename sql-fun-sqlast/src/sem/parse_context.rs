mod cast_info;
mod seq_info;
mod table_info;
mod type_info;
mod view_info;

use std::{
    io::{Read, Seek},
    path::Path,
};

pub use self::{
    cast_info::CastInfoRead,
    seq_info::{SequenceInfoRead, SequenceInfoWrite},
    table_info::{TableInfoRead, TableInfoWrite},
    type_info::{TypeInfoRead, TypeInfoWrite},
    view_info::{ViewInfoRead, ViewInfoWrite},
};

use crate::{
    StringSpan,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, Comment, CreateCompositeType, CreateDomain,
        CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateSchema, CreateSequence,
        CreateTable, DataSource, FullName, FunctionOverloadCollection, GetSourcePathWithSpanOpt,
        MutSourcePathWithSpan, OperatorDefinition, OperatorName, SchemaName, SemScalarExpr,
        TableName, TypeDefinition, TypeReference, VariableSet, ViewLike, ViewName,
        search_path::{expand_search_path, expand_search_path_inst},
        type_system::ArgumentBindingCollection,
    },
};
use sql_fun_core::{IVec, SqlFunArgs};

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
    /// get `CREATE INDEX` statement by index name
    fn get_index(&self, index_name: &str) -> Option<&CreateIndex>;
}

/// get function definition
pub trait FunctionInfoRead {
    /// get `CREATE FUNCTION` statement by function name
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection>;
}

/// store and update function info
pub trait FunctionInfoWrite
where
    Self: Sized,
{
    /// apply `create function` statement (implementation)
    fn apply_create_function_impl(
        self,
        name: &FullName,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError>;
}

/// get operator definition
pub trait OperatorInfoRead {
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
/// [`ParseContext`] presents mutate functions.
///
pub trait BaseParseContext:
    CastInfoRead
    + FunctionInfoRead
    + IndexInfoRead
    + OperatorInfoRead
    + SequenceInfoRead
    + TypeInfoRead
    + TableInfoRead
    + ViewInfoRead
    + SearchPathRead
    + GetSourcePathWithSpanOpt
    + std::fmt::Debug
{
    /// Resolved metadata for the database selected by the caller.
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata;

    /// get context args
    fn get_args(&self) -> &SqlFunArgs;

    /// get SQL file path
    fn get_source_file_path(&self) -> Option<&Path>;

    /// get type by name
    fn get_type(&self, key: &FullName) -> Option<&TypeDefinition> {
        expand_search_path(self, key, |k| self.get_type_impl(k))
    }

    /// get `CREATE TABLE` statement by table name
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        expand_search_path(self, name.full_name(), |key| self.get_table_impl(key))
    }

    /// get `CREATE VIEW` statement by view name
    fn get_view(&self, key: &ViewName) -> Option<&ViewLike> {
        expand_search_path(self, key.full_name(), |key| self.get_view_impl(key))
    }

    /// get `CREATE SEQUENCE` statement by sequence name
    fn get_sequence(&self, seq_name: &FullName) -> Option<&CreateSequence> {
        expand_search_path(self, seq_name, |key| self.get_sequence_impl(key))
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

    /// canonicalize a type reference
    fn canonicalize_type_ref(&self, type_ref: &TypeReference) -> Option<TypeReference> {
        let type_def = self.get_type(type_ref.full_name())?;
        type_def.full_name().map(|full_name| {
            TypeReference::concrete_type_ref(full_name, type_ref.is_array().unwrap_or(false))
        })
    }
}

/// Mutable diagnostic sink for semantic analysis.
pub trait DiagnosticContext {
    /// Report a recoverable analysis problem.
    ///
    /// Returns an error if the diagnostic cannot be accepted or rendered.
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError>;
}

/// modifiable schema
///
/// use in schema loading
///
pub trait ParseContext:
    SequenceInfoWrite
    + TypeInfoWrite
    + TableInfoWrite
    + ViewInfoWrite
    + FunctionInfoWrite
    + BaseParseContext
    + MutSourcePathWithSpan
    + DiagnosticContext
    + std::fmt::Debug
where
    Self: Sized,
{
    /// Replace the search path for this parse context without updating its base.
    fn set_search_path(&mut self, search_path: &[SchemaName]);

    /// Update location for the current statement.
    fn set_statement_span(&mut self, statement_span: StringSpan);

    /// Set the source access service used to render diagnostics.
    fn set_source_file(&mut self, source_access_service: Box<dyn SourceAccessService>);

    /// apply `create type` (composite type) to current schema
    fn apply_create_comp_type(
        mut self,
        create_comp_type: &CreateCompositeType,
    ) -> Result<Self, AnalysisError> {
        let Some(name) = create_comp_type.name() else {
            AnalysisError::raise_register_dynamic_type(create_comp_type)?;
            AnalysisError::never()
        };
        let name = self.apply_default_schema(name)?;
        self.insert_create_composite_type(&name, create_comp_type);
        Ok(self)
    }

    /// apply `create domain` to current schema
    fn apply_create_domain(mut self, create_domain: &CreateDomain) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_domain.name())?;
        self.insert_create_domain(&name, create_domain);
        Ok(self)
    }

    /// apply `comment` statement to current schema
    fn apply_comment(self, comment: &Comment) -> Result<Self, AnalysisError>;

    /// apply default schema when name not specify schema
    fn apply_default_schema(&self, name: &FullName) -> Result<FullName, AnalysisError> {
        let name = if name.schema().is_none() {
            let Some(schema) = self.get_default_schema() else {
                AnalysisError::raise_unexpected_none("context.get_default_schema")?
            };
            name.clone().set_schema(schema)
        } else {
            name.clone()
        };
        Ok(name)
    }

    /// apply `create type as enum` statement to  current schema
    fn apply_create_enum(mut self, create_enum_type: &CreateEnum) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_enum_type.name())?;
        self.insert_create_enum(&name, create_enum_type);
        Ok(self)
    }

    /// apply `create table` statement to current schema
    fn apply_create_table(mut self, create_table: &CreateTable) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_table.name().full_name())?;
        self.insert_create_table(&name, create_table);
        // when create table, define composite type as table
        self.insert_table_ref_type(&TableName::from(name));
        Ok(self)
    }

    /// apply `create extension` to current schema
    fn apply_create_extension(
        self,
        create_extension: &CreateExtension,
    ) -> Result<Self, AnalysisError>;

    /// apply `create function` statement to current schema
    fn apply_create_function(
        self,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_function.name())?;
        self.apply_create_function_impl(&name, create_function)
    }

    /// apply `set` statement to current schema
    fn apply_variable_set(self, variable_set: &VariableSet) -> Result<Self, AnalysisError>;

    /// apply `create schema` to current schema
    fn apply_create_schema(self, create_schema: &CreateSchema) -> Result<Self, AnalysisError>;

    /// apply `create index` statement to current schema
    fn apply_create_index(self, create_index: &CreateIndex) -> Result<Self, AnalysisError>;

    /// resolve table function call as data source
    fn resolve_table_function(
        &mut self,
        call: &crate::syn::FuncCall,
        func_name: FullName,
        alias_name: &AliasSpec,
        arg_exprs: IVec<SemScalarExpr>,
        arg_types: ArgumentBindingCollection,
    ) -> Result<DataSource, AnalysisError> {
        let Some(overloads) = self.get_function_by_name(&func_name) else {
            self.report_problem(AnalysisProblem::table_function_not_found(&func_name))?;
            return DataSource::undefined_table_function(call, arg_exprs, alias_name);
        };

        let Some(calling_overload) = overloads.resolve_overload(self, &arg_types) else {
            self.report_problem(AnalysisProblem::parameter_type_mismatch_for_table_function(
                &func_name, &arg_types,
            ))?;
            return DataSource::undefined_table_function(call, arg_exprs, alias_name);
        };

        Ok(DataSource::table_function(
            &calling_overload,
            arg_exprs,
            alias_name,
        ))
    }
}
