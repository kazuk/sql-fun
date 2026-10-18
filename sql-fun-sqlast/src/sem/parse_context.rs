mod cast_info;
pub(crate) mod conflict_info;
mod cursors;
mod rule_info;
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
    conflict_info::{CatalogNameLookup, ConflictArbiterInference, ConflictInfoRead},
    cursors::CursorInfo,
    rule_info::{RuleInfoRead, RuleInfoWrite},
    seq_info::{SequenceInfoRead, SequenceInfoWrite},
    table_info::{TableInfoRead, TableInfoWrite},
    type_info::{TypeInfoRead, TypeInfoWrite},
    view_info::{ViewInfoRead, ViewInfoWrite},
};

pub(crate) use self::cursors::CursorBindings;

use crate::{
    StringSpan,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, Comment, CreateCompositeType, CreateDomain,
        CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateSchema, CreateSequence,
        CreateTable, DataSource, FrozenBaseContext, FullName, FunctionOverloadCollection,
        GetSourcePathWithSpanOpt, MutSourcePathWithSpan, OperatorDefinition, OperatorName,
        SchemaName, SemScalarExpr, TableName, TypeDefinition, TypeReference, VariableSet, ViewLike,
        ViewName,
        search_path::{
            expand_relation_search_path, expand_search_path_inst, expand_type_search_path,
        },
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
    /// Enumerate indexes belonging to the qualified destination table.
    fn indexes_for_table(&self, table: &TableName) -> Vec<&CreateIndex>;

    /// Read an index by its exact qualified identity, without search-path expansion.
    fn get_index_impl(&self, index_name: &FullName) -> Option<&CreateIndex>;
}

/// get function definition
pub trait FunctionInfoRead {
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

    /// Look up an exact trigger identity in the effective context layer.
    fn trigger(&self, identity: &crate::sem::TriggerIdentity)
    -> Option<&crate::sem::CreateTrigger>;

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
        expand_search_path_inst(self, name, |key| {
            if self.get_table(&TableName::from(key.clone())).is_some()
                || self.get_view(&ViewName::from_full_name(key)).is_some()
            {
                Some(key.clone())
            } else {
                None
            }
        })
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

/// Common environment for SQL analysis and mutable schema loading.
///
/// Configuration, catalog services, diagnostic policy, and published results
/// come from this context. Local name scopes and inference graphs remain separate.
/// Implementations receive current args and explicitly selected metadata at
/// initialization; a frozen catalog does not supply their configuration.
///
pub trait ParseContext:
    RuleInfoWrite
    + SequenceInfoWrite
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
    /// Apply nested schema declarations as one publication group.
    /// Implementations retain diagnostics while discarding catalog writes when a
    /// nested declaration is rejected. The closure keeps using this ParseContext.
    fn apply_creation_group<F>(
        self,
        operation: F,
    ) -> Result<crate::AstAndContextPair<Self>, AnalysisError>
    where
        F: FnOnce(Self) -> Result<crate::AstAndContextPair<Self>, AnalysisError>;

    /// Effective creation identity, including explicitly requested temporary storage.
    fn creation_name(&self, name: &FullName, temporary: bool) -> Result<FullName, AnalysisError> {
        if temporary && name.schema().is_none() {
            let schema = if self.dialect() == crate::ast::Dialect::sqlite() {
                "temp"
            } else {
                "pg_temp"
            };
            Ok(name.clone().set_schema(SchemaName::from(schema)))
        } else {
            self.apply_default_schema(name)
        }
    }

    /// Validate creation against the exact shared relation namespace.
    /// Returns false for an accepted no-op or a diagnosed conflict.
    fn permits_relation_creation(
        &mut self,
        name: &FullName,
        kind: crate::sem::Kind,
        if_not_exists: bool,
        replace: bool,
    ) -> Result<bool, AnalysisError> {
        if self.dialect() == crate::ast::Dialect::sqlite()
            && name
                .name()
                .as_str()
                .to_ascii_lowercase()
                .starts_with("sqlite_")
        {
            let span = self
                .get_source_path_with_span()
                .map_or(StringSpan::default(), |source| *source.span());
            self.report_problem(AnalysisProblem::creation(
                crate::sem::problem::CreationProblem::named(name, "reserved SQLite name", &span),
            ))?;
            return Ok(false);
        }
        if let Some(existing) = self.object_kind(name) {
            if if_not_exists
                && (self.dialect() != crate::ast::Dialect::sqlite()
                    || kind.permits_sqlite_noop(existing))
            {
                return Ok(false);
            }
            if replace && kind == crate::sem::Kind::view() && existing == kind {
                return Ok(true);
            }
            let span = self
                .get_source_path_with_span()
                .map_or(StringSpan::default(), |source| *source.span());
            self.report_problem(AnalysisProblem::creation(
                crate::sem::problem::CreationProblem::relation(name, kind, existing, &span),
            ))?;
            return Ok(false);
        }
        if self.dialect() == crate::ast::Dialect::postgres()
            && kind.creates_row_type()
            && !self.permits_type_creation(name)?
        {
            return Ok(false);
        }
        Ok(true)
    }

    /// Validate a separate PostgreSQL type-name namespace without publishing a definition.
    fn permits_type_creation(&mut self, name: &FullName) -> Result<bool, AnalysisError> {
        if self.get_type_impl(name).is_some() {
            let span = self
                .get_source_path_with_span()
                .map_or(StringSpan::default(), |source| *source.span());
            self.report_problem(AnalysisProblem::creation(
                crate::sem::problem::CreationProblem::named(name, "type", &span),
            ))?;
            Ok(false)
        } else {
            Ok(true)
        }
    }
    /// Register a completed definition in this layer. Ownership is decided by DDL application.
    fn register_trigger(self, trigger: &crate::sem::CreateTrigger) -> Result<Self, AnalysisError>;
    /// Associate the supplied name with the exact target in this layer.
    fn associate_trigger(self, target: &FullName, name: &FullName) -> Result<Self, AnalysisError>;

    /// Dialect of the explicitly supplied metadata for this analysis.
    /// Engine identity and SQL dialect are independent metadata fields.
    /// Historical configuration retained by a frozen catalog is not consulted.
    fn dialect(&self) -> crate::ast::Dialect {
        crate::ast::Dialect::from(self.metadata())
    }

    /// Resolve an exact portal name in the current session scope.
    /// An absent binding is an unresolved cursor, including in an empty scope.
    /// Portal names are distinct from procedural refcursor variable names.
    fn resolve_cursor(&self, name: &str) -> Option<&CursorInfo>;

    /// Establish or replace a portal binding in the current session scope.
    /// Providers own storage and lifecycle bookkeeping; analysis only looks up bindings.
    /// Session bindings are independent of schema catalogs and are not frozen into them.
    fn bind_cursor(&mut self, name: &str, info: &CursorInfo);

    /// Remove a portal binding from the current session scope.
    fn remove_cursor(&mut self, name: &str);

    /// Bookkeeping for the current outer analysis, independent of local scopes.
    fn analysis_state(&self) -> &crate::sem::AnalysisState;

    /// Mutable bookkeeping for the current outer analysis.
    fn analysis_state_mut(&mut self) -> &mut crate::sem::AnalysisState;

    /// Start parameter type analysis for an independent statement or expression.
    /// Nested queries must not call this operation.
    fn begin_parameter_type_analysis(&mut self) {
        self.analysis_state_mut().begin();
        self.set_parameter_constraints(&crate::sem::ParameterConstraints::default());
    }

    /// Complete analysis and deliver ordered structured findings, even on error.
    ///
    /// Diagnostic rejection takes precedence over the analysis result and stops
    /// delivery. This is the completion boundary for future expectation checks;
    /// policy at emission sites is provided by this same context.
    ///
    /// # Errors
    /// Returns diagnostic delivery failures before the supplied analysis error.
    fn finish_analysis<T>(&mut self, result: Result<T, AnalysisError>) -> Result<T, AnalysisError> {
        let problems = self.analysis_state_mut().take_problems();
        for problem in problems {
            self.report_problem(problem)?;
        }
        result
    }

    /// Queue a structured finding in this analysis's diagnostic lifecycle.
    /// Direct semantic helper calls outside an active run deliver immediately.
    ///
    /// # Errors
    /// Returns diagnostic delivery errors for calls outside an active analysis.
    fn report_analysis_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        if self.analysis_state().active() {
            self.analysis_state_mut().report(problem);
            Ok(())
        } else {
            self.report_problem(problem)
        }
    }

    /// Whether parameter operands have been encountered in the current analysis.
    fn parameter_inference_needed(&self) -> bool {
        self.analysis_state().has_parameters()
    }

    /// Record a parameter operand without starting constraint collection.
    fn parameter(&mut self, position: u32) -> SemScalarExpr {
        self.analysis_state_mut().parameter();
        SemScalarExpr::parameter(
            position,
            usize::MAX,
            &crate::sem::InputTypeConstraint::any(),
        )
    }

    /// Infer parameters over an already name-resolved query, when required.
    ///
    /// # Errors
    /// Propagates the supplied result or constraint-analysis errors.
    fn infer_query_parameter_types(
        &mut self,
        result: Result<crate::sem::SelectStatement, AnalysisError>,
    ) -> Result<crate::sem::SelectStatement, AnalysisError> {
        let mut query = result?;
        if self.parameter_inference_needed() {
            let constraints = crate::sem::parameter_resolution::ParameterResolution::infer_query(
                &mut query, self,
            )?;
            self.set_parameter_constraints(&constraints);
        }
        Ok(query)
    }

    /// Infer parameters over an already name-resolved standalone expression.
    ///
    /// # Errors
    /// Propagates the supplied result or constraint-analysis errors.
    fn infer_expression_parameter_types(
        &mut self,
        result: Result<SemScalarExpr, AnalysisError>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut expression = result?;
        if self.parameter_inference_needed() {
            let constraints =
                crate::sem::parameter_resolution::ParameterResolution::infer_expression(
                    &mut expression,
                    self,
                )?;
            self.set_parameter_constraints(&constraints);
        }
        Ok(expression)
    }

    /// Constraints derived from the most recently reported semantic analysis.
    /// Parameter-free analyses skip inference and leave this result empty.
    /// These are independent of parameter metadata hints and Rust type mapping.
    /// Conflicts are reported through [`DiagnosticContext::report_problem`]; a
    /// returned analysis result does not imply satisfiable constraints.
    fn parameter_constraints(&self) -> &crate::sem::ParameterConstraints;

    /// Replace the current analysis constraints, including an empty result.
    fn set_parameter_constraints(&mut self, constraints: &crate::sem::ParameterConstraints);

    /// Consume this builder and share its completed catalog as an immutable base.
    ///
    /// Earlier successful edits, the effective search path, and final source
    /// provenance remain readable. Statement parameter constraints are discarded;
    /// inspect them before freezing. Diagnostic collection and source rendering
    /// services are also discarded; inspect problems before freezing or use a
    /// schema-file loader to retain diagnostics on its parsed result.
    ///
    /// Freezing consumes the builder; it cannot be reused afterwards:
    ///
    /// ```compile_fail
    /// use ::sql_fun_sqlast::sem::{ParseContext, SchemaFileContext};
    ///
    /// fn reuse_builder(mut builder: SchemaFileContext) {
    ///     let _frozen = builder.freeze();
    ///     builder.set_search_path(&[]);
    /// }
    /// ```
    fn freeze(self) -> FrozenBaseContext;

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
        if !self.permits_type_creation(&name)? {
            return Ok(self);
        }
        if !self.permits_relation_creation(
            &name,
            crate::sem::Kind::composite_type(),
            false,
            false,
        )? {
            return Ok(self);
        }
        self.insert_create_composite_type(&name, create_comp_type);
        Ok(self)
    }

    /// apply `create domain` to current schema
    fn apply_create_domain(mut self, create_domain: &CreateDomain) -> Result<Self, AnalysisError> {
        let name = self.apply_default_schema(create_domain.name())?;
        if !self.permits_type_creation(&name)? {
            return Ok(self);
        }
        self.insert_create_domain(&name, create_domain);
        Ok(self)
    }

    /// Register a role explicitly supplied by the caller's catalog inventory.
    fn insert_role(&mut self, role: &crate::sem::Role);

    /// Register an existing object of a class without a richer retained DDL model.
    /// This establishes existence, never an implicit consequence of ALTER OWNER.
    fn insert_owned_object(&mut self, definition: &crate::sem::OwnedObjectDefinition);

    /// Apply a resolved ownership change. Invalid outcomes leave the catalog unchanged.
    fn apply_alter_owner(self, alter_owner: &crate::sem::AlterOwner)
    -> Result<Self, AnalysisError>;

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
        if !self.permits_type_creation(&name)? {
            return Ok(self);
        }
        self.insert_create_enum(&name, create_enum_type);
        Ok(self)
    }

    /// apply `create table` statement to current schema
    fn apply_create_table(mut self, create_table: &CreateTable) -> Result<Self, AnalysisError> {
        let name = self.creation_name(create_table.name().full_name(), create_table.temporary())?;
        if !self.permits_relation_creation(
            &name,
            crate::sem::Kind::table(),
            create_table.mode().if_not_exists(),
            false,
        )? {
            return Ok(self);
        }
        let indexes = if self.dialect() == crate::ast::Dialect::postgres() {
            create_table.declared_indexes(&name)?
        } else {
            Vec::new()
        };
        for index in &indexes {
            if index.name() == &name {
                let span =
                    *crate::sem::GetSourcePathWithSpan::get_source_path_with_span(create_table)
                        .span();
                self.report_problem(AnalysisProblem::creation(
                    crate::sem::problem::CreationProblem::relation(
                        &name,
                        crate::sem::Kind::index(),
                        crate::sem::Kind::table(),
                        &span,
                    ),
                ))?;
                return Ok(self);
            }
            if !self.permits_relation_creation(
                index.name(),
                crate::sem::Kind::index(),
                false,
                false,
            )? {
                return Ok(self);
            }
        }
        self.insert_create_table(&name, create_table);
        // when create table, define composite type as table
        if self.dialect() == crate::ast::Dialect::postgres() {
            self.insert_table_ref_type(&TableName::from(name));
        }
        for index in indexes {
            self = self.apply_create_index(&index)?;
        }
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
            self.report_analysis_problem(AnalysisProblem::table_function_not_found(&func_name))?;
            return DataSource::undefined_table_function(call, arg_exprs, alias_name);
        };

        let Some(calling_overload) = overloads.resolve_overload(self, &arg_types) else {
            self.report_analysis_problem(
                AnalysisProblem::parameter_type_mismatch_for_table_function(&func_name, &arg_types),
            )?;
            return DataSource::undefined_table_function(call, arg_exprs, alias_name);
        };

        Ok(DataSource::table_function(
            &calling_overload,
            arg_exprs,
            alias_name,
        ))
    }
}
