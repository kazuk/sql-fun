//! Shared catalog interfaces and services for SQL analysis.

mod base_context;
mod base_parse_context;
mod cast_info;
mod conflict_info;
mod cursor_bindings;
mod diagnostic;
mod function_info;
mod index_info;
mod object_info;
mod operator_info;
mod ownership_info;
mod rule_info;
mod search_path;
mod sequence_info;
mod source;
mod table_info;
mod trigger_info;
mod type_hint;
mod type_info;
mod view_info;

pub use self::base_context::{BaseContext, PgBuiltInFunctions, PgBuiltinCasts, PgBuiltinOperators};
pub use self::base_parse_context::BaseParseContext;
pub use self::cast_info::CastInfoRead;
pub use self::conflict_info::ConflictInfoRead;
pub(crate) use self::cursor_bindings::CursorBindings;
pub use self::diagnostic::DiagnosticContext;
pub use self::function_info::{FunctionInfoRead, FunctionInfoWrite};
pub use self::index_info::IndexInfoRead;
pub use self::object_info::{ExtensionInfoRead, ObjectInfoRead};
pub use self::operator_info::OperatorInfoRead;
pub use self::ownership_info::{OwnershipInfoRead, RoleInfoRead};
pub use self::rule_info::{RuleInfoRead, RuleInfoWrite};
pub use self::search_path::SearchPathRead;
pub use self::sequence_info::{SequenceInfoRead, SequenceInfoWrite};
pub use self::source::{
    DefaultSourceAccessService, GetSourcePathWithSpan, GetSourcePathWithSpanOpt,
    MutSourcePathWithSpan, SourceAccessService,
};
pub use self::table_info::{TableInfoRead, TableInfoWrite};
pub use self::trigger_info::{TriggerInfoRead, TriggerInfoWrite};
pub(crate) use self::type_hint::ParameterTypeHints;
pub use self::type_hint::{ParameterTypeHintRead, ParameterTypeHintWrite};
pub use self::type_info::{TypeInfoRead, TypeInfoWrite};
pub use self::view_info::{
    ViewDeleteInfoRead, ViewInfoRead, ViewInfoWrite, ViewInsertInfoRead, ViewMergeInfoRead,
    ViewUpdateInfoRead,
};

use sql_fun_core::{IVec, analysis_problem::AnalysisProblem};

use crate::{
    StringSpan,
    sem::{
        AliasSpec, AlterIndex, AlterOwner, AnalysisError, Comment, CreateCompositeType,
        CreateDomain, CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateSchema,
        CreateTable, CursorInfo, DataSource, FrozenBaseContext, FullName, Kind,
        OwnedObjectDefinition, Role, SchemaName, SemScalarExpr, TableName, VariableSet,
        problem::CreationProblem, type_system::ArgumentBindingCollection,
    },
};

/// Common environment for SQL analysis and mutable schema loading.
///
/// Configuration, catalog services, diagnostic policy, and published results
/// come from this context. Local name scopes and inference graphs remain separate.
/// Implementations receive current args and explicitly selected metadata at
/// initialization; a frozen catalog does not supply their configuration.
///
pub trait ParseContext:
    ParameterTypeHintRead
    + ParameterTypeHintWrite
    + RuleInfoWrite
    + TriggerInfoWrite
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
        kind: Kind,
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
            self.report_problem(AnalysisProblem::creation(CreationProblem::named(
                name,
                "reserved SQLite name",
                &span,
            )))?;
            return Ok(false);
        }
        if let Some(existing) = self.object_kind(name) {
            if if_not_exists
                && (self.dialect() != crate::ast::Dialect::sqlite()
                    || kind.permits_sqlite_noop(existing))
            {
                return Ok(false);
            }
            if replace && kind == Kind::view() && existing == kind {
                return Ok(true);
            }
            let span = self
                .get_source_path_with_span()
                .map_or_else(StringSpan::default, |source| *source.span());
            self.report_problem(AnalysisProblem::creation(CreationProblem::relation(
                name, kind, existing, &span,
            )))?;
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
            self.report_problem(AnalysisProblem::creation(CreationProblem::named(
                name, "type", &span,
            )))?;
            Ok(false)
        } else {
            Ok(true)
        }
    }
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
        let constraint = if let Some(ty) = self.get_parameter_typehint(position as usize) {
            crate::sem::InputTypeConstraint::exact(&ty)
        } else {
            crate::sem::InputTypeConstraint::any()
        };
        SemScalarExpr::parameter(position, usize::MAX, &constraint)
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
        } else {
            query.expand_pending_stars(self)?;
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
        expression.complete_grouping(self)?;
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
    /// These retain explicit SQL input types but do not perform Rust type mapping.
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
    /// use ::sql_fun_sqlast::{context::ParseContext, sem::SchemaFileContext};
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
        if !self.permits_relation_creation(&name, Kind::composite_type(), false, false)? {
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
    fn insert_role(&mut self, role: &Role);

    /// Register an existing object of a class without a richer retained DDL model.
    /// This establishes existence, never an implicit consequence of ALTER OWNER.
    fn insert_owned_object(&mut self, definition: &OwnedObjectDefinition);

    /// Apply a resolved ownership change. Invalid outcomes leave the catalog unchanged.
    fn apply_alter_owner(self, alter_owner: &AlterOwner) -> Result<Self, AnalysisError>;

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
            Kind::table(),
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
                let span = *GetSourcePathWithSpan::get_source_path_with_span(create_table).span();
                self.report_problem(AnalysisProblem::creation(CreationProblem::relation(
                    &name,
                    Kind::index(),
                    Kind::table(),
                    &span,
                )))?;
                return Ok(self);
            }
            if !self.permits_relation_creation(index.name(), Kind::index(), false, false)? {
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

    /// Publish the recorded atomic ALTER INDEX replacement; rejected or skipped statements do nothing.
    fn apply_alter_index(self, index: &AlterIndex) -> Result<Self, AnalysisError>;

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
