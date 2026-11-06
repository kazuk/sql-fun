//! Catalog contexts, shared interfaces, and services for SQL analysis.

mod base_context;
mod base_parse_context;
mod cast_info;
mod conflict_info;
mod conflict_masters;
mod cursor_bindings;
mod diagnostic;
mod frozen_base_context;
mod function_info;
mod index_info;
mod object_info;
mod operator_info;
pub(crate) mod ownership;
mod ownership_info;
mod rule_info;
mod schema_file_context;
mod search_path;
mod sequence_info;
mod source;
mod table_info;
mod trigger_info;
mod type_hint;
mod type_info;
mod view_info;

pub use self::{
    base_context::{BaseContext, PgBuiltInFunctions, PgBuiltinCasts, PgBuiltinOperators},
    base_parse_context::BaseParseContext,
    cast_info::CastInfoRead,
    conflict_info::ConflictInfoRead,
    diagnostic::DiagnosticContext,
    frozen_base_context::FrozenBaseContext,
    function_info::{FunctionInfoRead, FunctionInfoWrite},
    index_info::IndexInfoRead,
    object_info::{ExtensionInfoRead, ObjectInfoRead},
    operator_info::OperatorInfoRead,
    ownership_info::{OwnershipInfoRead, RoleInfoRead},
    rule_info::{RuleInfoRead, RuleInfoWrite},
    schema_file_context::SchemaFileContext,
    search_path::SearchPathRead,
    sequence_info::{SequenceInfoRead, SequenceInfoWrite},
    source::{
        DefaultSourceAccessService, GetSourcePathWithSpan, GetSourcePathWithSpanOpt,
        MutSourcePathWithSpan, SourceAccessService,
    },
    table_info::{TableInfoRead, TableInfoWrite},
    trigger_info::{TriggerInfoRead, TriggerInfoWrite},
    type_hint::{ParameterTypeHintRead, ParameterTypeHintWrite},
    type_info::{TypeInfoRead, TypeInfoWrite},
    view_info::{
        ViewDeleteInfoRead, ViewInfoRead, ViewInfoWrite, ViewInsertInfoRead, ViewMergeInfoRead,
        ViewUpdateInfoRead,
    },
};
pub(crate) use self::{
    conflict_masters::ConflictMasters, cursor_bindings::CursorBindings,
    type_hint::ParameterTypeHints,
};

use crate::{
    AstAndContextPair, CacheContext, StringSpan,
    ast::Dialect,
    names::{FullName, SchemaName, TableName},
    sem::{
        AlterIndex, AlterOwner, AnalysisError, AnalysisState, Comment, CreateCompositeType,
        CreateDomain, CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateSchema,
        CreateTable, CursorInfo, InputTypeConstraint, Kind, OwnedObjectDefinition,
        ParameterConstraints, Role, SamplingMethodLookup, SelectStatement, SemScalarExpr,
        VariableSet, parameter_resolution::ParameterResolution, problem::CreationProblem,
    },
};

use sql_fun_core::analysis_problem::AnalysisProblem;

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
    ) -> Result<AstAndContextPair<Self>, AnalysisError>
    where
        F: FnOnce(Self) -> Result<AstAndContextPair<Self>, AnalysisError>;

    /// Effective creation identity, including explicitly requested temporary storage.
    fn creation_name(&self, name: &FullName, temporary: bool) -> Result<FullName, AnalysisError> {
        if temporary && name.schema().is_none() {
            let schema = if self.dialect() == Dialect::sqlite() {
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
        if self.dialect() == Dialect::sqlite()
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
                && (self.dialect() != Dialect::sqlite() || kind.permits_sqlite_noop(existing))
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
        if self.dialect() == Dialect::postgres()
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
    fn dialect(&self) -> Dialect {
        Dialect::from(self.metadata())
    }

    /// Resolve a TABLESAMPLE handler using the active qualification/search path rules.
    /// Providers must distinguish conclusive absence from unavailable metadata and
    /// return verified `(internal) RETURNS tsm_handler` facts on success. No source
    /// discovery is authorized here. Production acquisition is tracked by #895.
    fn lookup_sampling_method(&self, _name: &FullName) -> SamplingMethodLookup {
        SamplingMethodLookup::unavailable()
    }

    /// Resolve an explicit SQLite COLLATE name from the loaded analysis metadata.
    /// This is separate from conflict-target catalog services. Registration is
    /// not implemented yet, so every name is currently absent, including builtins.
    /// No connection or ambient registry is inspected.
    fn resolve_sqlite_collation(&self, _name: &FullName) -> Option<FullName> {
        None
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
    fn analysis_state(&self) -> &AnalysisState;

    /// Mutable bookkeeping for the current outer analysis.
    fn analysis_state_mut(&mut self) -> &mut AnalysisState;

    /// Start parameter type analysis for an independent statement or expression.
    /// Nested queries must not call this operation.
    fn begin_parameter_type_analysis(&mut self) {
        self.analysis_state_mut().begin();
        self.set_parameter_constraints(&ParameterConstraints::default());
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
            InputTypeConstraint::exact(&ty)
        } else {
            InputTypeConstraint::any()
        };
        SemScalarExpr::parameter(position, usize::MAX, &constraint)
    }

    /// Infer parameters over an already name-resolved query, when required.
    ///
    /// # Errors
    /// Propagates the supplied result or constraint-analysis errors.
    fn infer_query_parameter_types(
        &mut self,
        result: Result<SelectStatement, AnalysisError>,
        cache: &mut CacheContext,
    ) -> Result<SelectStatement, AnalysisError> {
        let mut query = result?;
        if self.parameter_inference_needed() {
            let constraints = ParameterResolution::infer_query(&mut query, self, cache)?;
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
        cache: &mut CacheContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut expression = result?;
        expression.complete_grouping(self)?;
        if self.parameter_inference_needed() {
            let constraints = ParameterResolution::infer_expression(&mut expression, self, cache)?;
            self.set_parameter_constraints(&constraints);
        }
        Ok(expression)
    }

    /// Constraints derived from the most recently reported semantic analysis.
    /// Parameter-free analyses skip inference and leave this result empty.
    /// These retain explicit SQL input types but do not perform Rust type mapping.
    /// Conflicts are reported through [`DiagnosticContext::report_problem`]; a
    /// returned analysis result does not imply satisfiable constraints.
    fn parameter_constraints(&self) -> &ParameterConstraints;

    /// Replace the current analysis constraints, including an empty result.
    fn set_parameter_constraints(&mut self, constraints: &ParameterConstraints);

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
    /// use ::sql_fun_sqlast::context::{ParseContext, SchemaFileContext};
    ///
    /// fn reuse_builder(mut builder: SchemaFileContext) {
    ///     let _frozen = builder.freeze();
    ///     builder.set_search_path(&[]);
    /// }
    /// ```
    fn freeze(self) -> FrozenBaseContext;

    /// Retain the effective immutable catalog for later assignment inspection.
    /// Query-local diagnostics, bindings and inferred constraints are excluded.
    fn retain_catalog(&self) -> FrozenBaseContext;

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
            create_comp_type.raise_registration_error()?;
            AnalysisError::never()
        };
        if self.dialect() == Dialect::postgres() && !create_comp_type.declarations_can_apply() {
            return Ok(self);
        }
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
        if self.dialect() == Dialect::postgres() && !create_domain.declarations_can_apply() {
            return Ok(self);
        }
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
        if self.dialect() == Dialect::postgres() && !create_table.declarations_can_apply() {
            return Ok(self);
        }
        let name = self.creation_name(create_table.name().full_name(), create_table.temporary())?;
        if !self.permits_relation_creation(
            &name,
            Kind::table(),
            create_table.mode().if_not_exists(),
            false,
        )? {
            return Ok(self);
        }
        let indexes = if self.dialect() == Dialect::postgres() {
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
        if !create_table.permits_serial_creation(&mut self, &indexes)? {
            return Ok(self);
        }
        for sequence in create_table.serial_sequences() {
            self = self.apply_create_sequence(sequence)?;
        }
        self.insert_create_table(&name, create_table);
        // when create table, define composite type as table
        if self.dialect() == Dialect::postgres() {
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
}

#[cfg(test)]
mod test_subsequent_analysis_replaces_context_constraints {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, public_analysis::analyze},
    };

    #[test]
    fn subsequent_analysis_replaces_context_constraints() {
        let mut cache_context = create_cache_context();
        let first = analyze("select $7 + 1 as n", Dialect::postgres()).unwrap();
        assert!(
            first
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_some()
        );
        let second = SemAst::from_ast(
            &parse_statements("select 1 as n", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            first.into_context(),
            &mut cache_context,
        )
        .unwrap();
        assert!(
            second
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_none()
        );
        assert!(
            second
                .context()
                .parameter_constraints()
                .result_column(0)
                .is_none()
        );
    }

    #[test]
    fn parameter_state_restarts_after_parameter_free_analysis() {
        let mut cache_context = create_cache_context();
        let first = analyze("select $7 + 1", Dialect::postgres()).unwrap();
        let plain = SemAst::from_ast(
            &parse_statements("select 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            first.into_context(),
            &mut cache_context,
        )
        .unwrap();
        let third = SemAst::from_ast(
            &parse_statements("select $2 + 1", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            plain.into_context(),
            &mut cache_context,
        )
        .unwrap();
        assert!(
            third
                .context()
                .parameter_constraints()
                .parameter(7)
                .is_none()
        );
        assert!(
            third
                .context()
                .parameter_constraints()
                .parameter(2)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_diagnostic_sink_failure_still_aborts_parameter_analysis {
    use crate::{
        ast::Dialect,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, public_analysis::context},
    };

    #[test]
    fn diagnostic_sink_failure_still_aborts_parameter_analysis() {
        let mut cache_context = create_cache_context();
        let mut context = context();
        context.reject_diagnostics_after(0);
        let error = SemAst::from_ast(
            &parse_statements("select $1 + 1, lower($1)", Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .err()
        .expect("diagnostic reporting must fail");
        assert_eq!(error.to_string(), "diagnostic sink rejected a problem");
    }
}

#[cfg(test)]
mod test_rejected_serial_does_not_publish_objects {
    use crate::{
        context::{SequenceInfoRead, TableInfoRead},
        names::FullName,
        test_helpers::{ownership::apply_ownership_sql, serial},
    };

    #[rstest::rstest]
    #[case::nullable("serial NULL", "serial_nullable")]
    #[case::default("serial DEFAULT 1", "serial_default_conflict")]
    #[case::identity("serial GENERATED ALWAYS AS IDENTITY", "serial_identity_conflict")]
    #[case::array("serial[]", "serial_array")]
    #[case::modifier("serial(3)", "serial_modifier")]
    fn rejected_serial_does_not_publish_objects(
        #[case] declaration: &str,
        #[case] expected_code: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            &format!("CREATE TABLE users(id {declaration})"),
        )?;
        assert!(
            !pair
                .ast()
                .as_create_table()
                .unwrap()
                .declarations_can_apply()
        );
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(context.problems().iter().any(|p| p.code() == expected_code));
        let context = apply_ownership_sql(context, "CREATE TABLE good(id integer)")?.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "good"))
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn index_conflict_leaves_no_serial_sequence() -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            "CREATE TABLE users(id serial CONSTRAINT users_id_seq PRIMARY KEY)",
        )?;
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(
            context
                .problems()
                .iter()
                .any(|p| p.code() == "conflicting_creation")
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_parameter_hint_initialization {

    use crate::{
        context::{ParameterTypeHintWrite, ParseContext},
        sem::PgBuiltInType,
        test_helpers::type_hint::context,
    };

    #[test]
    fn hinted_type_is_assigned_before_constraint_collection() {
        let mut context = context();
        context.setup_typehinting(&[Some("int4")]).unwrap();
        context.begin_parameter_type_analysis();
        assert_eq!(context.parameter(1).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.parameter(2).get_type(), None);
    }
}

#[cfg(test)]
pub(crate) use self::frozen_base_context::forward_catalog_reads;
