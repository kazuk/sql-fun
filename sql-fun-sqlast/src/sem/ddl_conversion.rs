//! Destination-owned conversion of common DDL syntax and catalog changes.

mod alter_index;
mod check;
mod column_attributes;
mod column_constraint;
mod column_rename;
mod serial;
mod table_rename;

use crate::ast::ddl::{
    AlterConstraintFlags, AlterDefinition, CompositeField, CompositeType, Domain, Function,
    FunctionParameter, Index, ParameterMode, Schema, TableAs,
};
use crate::ast::ddl::{AlterOperation, FunctionOption, PostgresAlterCommand};
use crate::ast::{
    AlterRelation, AlterRelationKind, ColumnConstraint as AstColumnConstraint, ForeignReference,
    Table,
};
use crate::sem::{AstConversion, convert};

use crate::{
    CacheContext,
    ast::{AlterCommand, Column as AstColumn, IndexExpression, TableConstraint},
    sem::{AlterSequence, ColumnRename, ForeignKeyTarget},
};

use crate::{
    AstAndContextPair,
    context::ParseContext,
    names::{ColumnName, FullName, LocalName, TableName, ViewName},
    sem::{
        AliasSpec, AlterIndex, AlterObjSubCommand, AlterTable, AlterView, AnalysisError,
        AnalysisProblem, AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint, CollateColumnConstraint, ColumnCollection, ColumnConstraint,
        ColumnDefinition, ColumnNullabilityConstraint, CompositeFieldCollection,
        CompositeFieldDefinition, Constraint, ConstraintNames, CreateCompositeType, CreateDomain,
        CreateFunction, CreateIndex, CreateSchema, CreateTable, CreateTableAs, CreateTableMode,
        DataSource, DefaultColumnConstraint, ExclusionColumnConstraint, ForeignKeyColumnConstraint,
        ForeignKeyConstraint, FromClause, FunctionOverloadCollection, FunctionParam,
        GeneratedColumnConstraint, IdentityColumnConstraint, IndexElement, IndexElementCollection,
        IndexElementWithOrder, Kind, OverloadVariant, PrimaryKeyColumnConstraint,
        PrimaryKeyConstraint, ResolutionFailure, ScalarAnalysisContext, SemAst, SemScalarExpr,
        SortOrder, SourcePathWithSpan, TypeCategory, TypeReference, UniqueColumnConstraint,
        UniqueConstraint, WithClause,
        problem::{CreationProblem, ViewProblem},
        search_path::expand_relation_search_path,
    },
};

use crate::ast::{Dialect, Expr, Name};

impl AstConversion<'_, AlterRelation> {
    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let source = context.get_source_path_with_span().cloned();
        self.resolve_and_apply_with_source(context, source.as_ref())
    }

    pub(crate) fn resolve_and_apply_at<TContext: ParseContext>(
        &self,
        context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        self.resolve_and_apply_with_source(context, Some(source))
    }

    pub(crate) fn resolve_and_apply_with_source<TContext: ParseContext>(
        &self,
        mut context: TContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !self.check_engine_features(&mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let changes = self.view_default_changes(&mut context);
        let Some(changes) = ResolutionFailure::finish(changes, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        if let Some(change) = changes {
            let context = context.apply_alter_view(&change)?;
            return Ok(AstAndContextPair::new(SemAst::alter_view(&change), context));
        }
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(&mut context, source);
        let statement = context.finish_analysis(result)?;
        let context = statement.apply_relation_alter(context)?;
        Ok(AstAndContextPair::new(statement, context))
    }

    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        if self.ast.kind.supports_table_column_attributes() && self.has_column_rename() {
            return self.resolve_column_rename(context, source);
        }
        if self.ast.kind.supports_table_column_attributes() && self.has_table_rename() {
            return self.resolve_table_rename(context, source);
        }
        if self.ast.kind.supports_table_column_attributes() && self.has_column_attributes() {
            return self.resolve_column_attributes(context, source);
        }
        convert(&self.ast.kind).resolve(
            context,
            &convert(&self.ast.name).full_name()?,
            &self.ast.commands,
            source,
            self.ast.if_exists,
        )
    }
}

impl AstConversion<'_, AlterRelationKind> {
    pub(crate) fn resolve(
        self,
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        source: Option<&SourcePathWithSpan>,
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        match self.ast {
            AlterRelationKind::Table => Self::resolve_table(context, name, syntax_commands, source),
            AlterRelationKind::View => Self::resolve_view(context, name, syntax_commands, false),
            AlterRelationKind::MaterializedView => {
                Self::resolve_view(context, name, syntax_commands, true)
            }
            AlterRelationKind::Sequence => Self::resolve_sequence(context, name, syntax_commands),
            AlterRelationKind::Index => {
                Self::resolve_index(context, name, syntax_commands, if_exists)
            }
            AlterRelationKind::ForeignTable | AlterRelationKind::CompositeType => todo!(),
        }
    }

    pub(crate) fn resolve_index(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        let mut operations = Vec::new();
        for command in syntax_commands {
            operations.push(convert(command).resolve_index(context)?);
        }
        let statement = AlterIndex::resolve(context, name, if_exists, &operations)?;
        Ok(SemAst::alter_index(&statement))
    }

    pub(crate) fn resolve_table(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        let table = TableName::from(name.clone());
        if context.get_table(&table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&table))?;
        }
        let mut commands = Vec::new();
        for command in syntax_commands {
            commands.push(convert(command).resolve(context, &table)?);
        }
        let mut statement = AlterTable::new(&table, &commands, source);
        statement.validate_check_names(context)?;
        Ok(SemAst::alter_table(&statement))
    }

    pub(crate) fn resolve_view(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        materialized: bool,
    ) -> Result<SemAst, AnalysisError> {
        let view = ViewName::from_full_name(name);
        let valid = context
            .get_view(&view)
            .is_some_and(|definition| definition.is_materialized() == materialized);
        if !valid {
            context.report_analysis_problem(AnalysisProblem::view(
                &ViewProblem::invalid_target(name),
            ))?;
            return Ok(SemAst::unresolved_statement());
        }
        let commands = syntax_commands
            .iter()
            .map(|command| convert(command).resolve_owner(context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_view(&AlterView::new(&view, &commands)))
    }

    pub(crate) fn resolve_sequence(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
    ) -> Result<SemAst, AnalysisError> {
        if context.get_sequence(name).is_none() {
            context.report_analysis_problem(AnalysisProblem::sequence_not_found(name))?;
        }
        let commands = syntax_commands
            .iter()
            .map(|command| convert(command).resolve_owner(context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_sequence(&AlterSequence::new(name, &commands)))
    }
}

impl AstConversion<'_, AlterCommand> {
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(AlterObjSubCommand, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(&mut context, table);
        let command = context.finish_analysis(result)?;
        Ok((command, context))
    }
    pub(crate) fn resolve_owner(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        match self.ast {
            AlterCommand::Postgres(command) => {
                convert::<PostgresAlterCommand>(command).resolve_owner(context)
            }
            AlterCommand::Owner(owner) => Ok(AlterObjSubCommand::resolved_owner(
                &convert(owner).resolve_owner(context)?,
            )),
            _ => todo!(),
        }
    }

    pub(crate) fn missing_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        if let Some(definition) = context.get_table(table)
            && !definition.has_column(column)
        {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                table, column,
            ))?;
        }
        Ok(())
    }

    pub(crate) fn existing_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        if let Some(definition) = context.get_table(table)
            && definition.has_column(column)
        {
            context
                .report_analysis_problem(AnalysisProblem::column_already_exists(table, column))?;
        }
        Ok(())
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        match self.ast {
            AlterCommand::Postgres(command) => {
                convert::<PostgresAlterCommand>(command).resolve(context, table)
            }
            AlterCommand::Owner(_) => self.resolve_owner(context),
            AlterCommand::Default(name, expr) => Self::resolve_default(context, table, name, expr),
            AlterCommand::Constraint(constraint) => {
                Self::resolve_constraint(context, table, constraint)
            }
            AlterCommand::Cluster(name) => Self::resolve_cluster(context, table, name),
            AlterCommand::Rename(name) => Self::resolve_rename(context, name),
            AlterCommand::AddColumn(column) => Self::resolve_add_column(context, table, column),
            AlterCommand::AlterColumn(name, column) if context.dialect() == Dialect::sqlite() => {
                Self::resolve_alter_column(context, table, name, column)
            }
            AlterCommand::RenameColumn(from, to) => {
                Self::resolve_rename_column(context, table, from, to)
            }
            AlterCommand::DropColumn(name) if context.dialect() == Dialect::sqlite() => {
                Self::resolve_drop_column(context, table, name)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn resolve_default(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        expr: &Expr,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        let expr = convert(expr).resolve_in(context, &ScalarAnalysisContext::default())?;
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::column_default(&name, &expr))
    }

    pub(crate) fn resolve_constraint(
        context: &mut impl ParseContext,
        table: &TableName,
        constraint: &TableConstraint,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        // Constraint-created indexes need the matched key even when the stored
        // table declaration remains unqualified.
        let resolved_table = context.resolve_table_name(table);
        let table = resolved_table.as_ref().unwrap_or(table);
        Ok(AlterObjSubCommand::add_constraint(
            &convert(constraint).resolve(context, table)?,
        ))
    }

    pub(crate) fn resolve_cluster(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let Some(resolved_table) = context.resolve_table_name(table) else {
            // The enclosing ALTER TABLE resolver already reports the absent table.
            return Ok(AlterObjSubCommand::cluster_on(name));
        };
        let index_name = FullName::new(
            &resolved_table.full_name().catalog().cloned(),
            &resolved_table.full_name().schema().cloned(),
            &LocalName::from(name),
        );
        if context.get_index(&index_name).is_none() {
            context.report_analysis_problem(AnalysisProblem::index_not_found(name))?;
        }
        Ok(AlterObjSubCommand::cluster_on(name))
    }

    pub(crate) fn resolve_rename(
        context: &mut impl ParseContext,
        name: &Name,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = TableName::from(convert(name).full_name()?);
        if context.get_table(&name).is_some() {
            context.report_analysis_problem(AnalysisProblem::relation_already_exists(&name))?;
        }
        Ok(AlterObjSubCommand::rename_to(&name))
    }

    pub(crate) fn resolve_add_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &AstColumn,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let column = convert(column).resolve_standalone(context, table)?;
        if let Some(name) = column.name() {
            Self::existing_column(context, table, name)?;
        }
        Ok(AlterObjSubCommand::add_column(&column))
    }

    pub(crate) fn resolve_alter_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        column: &AstColumn,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::alter_column(
            &name,
            &convert(column).resolve(context, table)?,
        ))
    }

    pub(crate) fn resolve_rename_column(
        context: &mut impl ParseContext,
        table: &TableName,
        from: &str,
        to: &str,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let (_, rename) = ColumnRename::resolve(
            context,
            table,
            &ColumnName::from(from),
            &ColumnName::from(to),
            false,
        )?;
        Ok(AlterObjSubCommand::analyzed_column_rename(&rename))
    }

    pub(crate) fn resolve_drop_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::drop_column(&name))
    }
}

impl AstConversion<'_, PostgresAlterCommand> {
    pub(crate) fn resolve_owner(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        if let Some(legacy) = &self.ast.legacy {
            convert::<AlterCommand>(legacy).resolve_owner(context)
        } else {
            todo!()
        }
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        if convert(&self.ast.operation)
            .column_attribute_phase()
            .is_some()
        {
            return self.resolve_single_column_attribute(context, table);
        }
        if let Some(legacy) = &self.ast.legacy {
            convert::<AlterCommand>(legacy).resolve(context, table)
        } else {
            todo!()
        }
    }
}

impl AstConversion<'_, TableConstraint> {
    pub(crate) fn resolve_declared_key(
        &self,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Constraint>, AnalysisError> {
        if !self.ast.constraint.is_unique_key() {
            return Ok(None);
        }
        if self.ast.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let keys = self.column_names();
        table.check_reference_columns(context, table.name(), &keys)?;
        convert(&self.ast.constraint)
            .resolve_table(
                context,
                table.name(),
                self.ast.name.as_deref().unwrap_or_default(),
                &keys,
            )
            .map(Some)
    }
}

impl AstConversion<'_, TableConstraint> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Constraint, AnalysisError> {
        if self.is_check() {
            let definition = context.get_table(table).cloned();
            return self
                .resolve_check(context, table, definition.as_ref(), true)
                .map(Constraint::Check);
        }
        let keys = self.column_names();
        if context.get_table(table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(table))?;
        }
        for key in &keys {
            AstConversion::<AlterCommand>::missing_column(context, table, key)?;
        }
        if self.ast.constraint.is_unique_key() && self.ast.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let name = self.ast.name.as_deref().unwrap_or_default();
        let mut constraint =
            convert(&self.ast.constraint).resolve_table(context, table, name, &keys)?;
        constraint.set_foreign_key_delete_columns(&self.delete_column_names());
        Ok(constraint)
    }
}

impl AstConversion<'_, AstColumnConstraint> {
    pub(crate) fn has_catalog_name(&self, context: &impl ParseContext) -> bool {
        match self.ast {
            AstColumnConstraint::PrimaryKey(..)
            | AstColumnConstraint::Unique(_)
            | AstColumnConstraint::Check(_)
            | AstColumnConstraint::ForeignKey(_)
            | AstColumnConstraint::Exclusion => true,
            AstColumnConstraint::Nullability(true, ..) => Self::supports_named_not_null(context),
            _ => false,
        }
    }
    fn supports_named_not_null(context: &impl ParseContext) -> bool {
        let Some(version) = context.metadata().engine_version() else {
            return false;
        };
        let Ok(version) = version
            .to_string()
            .parse::<::sql_fun_core::PostgresVersion>()
        else {
            return false;
        };
        version.major() >= 18
    }
}

impl AstConversion<'_, AstColumnConstraint> {
    pub(crate) fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Constraint, AnalysisError> {
        match self.ast {
            AstColumnConstraint::PrimaryKey(_, _) => {
                Ok(Self::resolve_table_primary_key(table, name, keys))
            }
            AstColumnConstraint::Unique(_) => Ok(Self::resolve_table_unique(table, name, keys)),
            AstColumnConstraint::ForeignKey(reference) => {
                convert(reference).resolve_table(context, table, name, keys)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn resolve_table_primary_key(
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Constraint {
        Constraint::primary_key(&PrimaryKeyConstraint::new(name, table, keys))
    }

    pub(crate) fn resolve_table_unique(
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Constraint {
        Constraint::unique(&UniqueConstraint::new(name, table, keys))
    }
}

impl AstConversion<'_, ForeignReference> {
    pub(crate) fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Constraint, AnalysisError> {
        self.resolve_table_in(context, table, name, keys, None)
    }

    pub(crate) fn resolve_table_in(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
        pending: Option<&CreateTable>,
    ) -> Result<Constraint, AnalysisError> {
        #[cfg(feature = "sqlite")]
        for column in keys {
            self.report_ignored_matches(context, table, column)?;
        }
        let requested = TableName::from(convert(&self.ast.table).full_name()?);
        let explicit = self.referenced_columns();
        let (binding, target_columns, target_valid) = if let Some(pending) = pending {
            ForeignKeyTarget::resolve_declared_reference(
                context,
                &requested,
                explicit.as_deref(),
                pending,
            )?
        } else {
            let binding = ForeignKeyTarget::resolve_relation(context, &requested)?;
            let (columns, valid) =
                binding.referenced_columns(context, &requested, explicit.as_deref())?;
            (binding, columns, valid)
        };
        let source_valid = if let Some(pending) = pending {
            pending.check_reference_columns(context, table, keys)?
        } else {
            true
        };
        let mut constraint =
            ForeignKeyConstraint::new(name, Some(table), binding.relation(), keys, &target_columns)
                .with_attributes(
                    self.ast.match_type.as_ref(),
                    self.ast.delete_action.as_ref(),
                    self.ast.update_action.as_ref(),
                    self.ast.deferrability.as_ref(),
                    self.ast.validity.as_ref(),
                );
        constraint.restrict_reference_application(source_valid && target_valid);
        Ok(Constraint::foreign(&constraint))
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn report_ignored_matches(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        for name in &self.ast.ignored_matches {
            context.report_analysis_problem(
                AnalysisProblem::sqlite_ignores_match_option_in_fk_constraint(table, column, name),
            )?;
        }
        Ok(())
    }

    pub(crate) fn referenced_columns(&self) -> Option<Vec<ColumnName>> {
        if self.ast.columns.is_empty() {
            None
        } else {
            Some(
                self.ast
                    .columns
                    .iter()
                    .map(|key| ColumnName::from(key.as_str()))
                    .collect(),
            )
        }
    }
}

impl AstConversion<'_, Table> {
    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        self.resolve_and_apply_at(context, &source)
    }

    pub(crate) fn constraint_names(&self, context: &impl ParseContext) -> ConstraintNames {
        let mut names = ConstraintNames::default();
        for definition in &self.ast.constraints {
            if !definition.is_check() {
                names.reserve(definition.name.as_deref().unwrap_or_default());
            }
        }
        for column in &self.ast.columns {
            convert(column).reserve_constraint_names(context, &mut names);
        }
        names
    }
    pub(crate) fn resolve_and_apply_at<TContext: ParseContext>(
        &self,
        mut context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !self.check_engine_features(&mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let name =
            context.creation_name(&convert(&self.ast.name).full_name()?, self.ast.temporary)?;
        if !context.permits_relation_creation(
            &name,
            Kind::table(),
            self.ast.mode.if_not_exists(),
            false,
        )? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let mut names = self.constraint_names(&context);
        if context.dialect() == Dialect::postgres()
            && let Some(constraint) = names.first_duplicate()
        {
            context.report_problem(AnalysisProblem::creation(CreationProblem::constraint(
                &name,
                &FullName::from_local_name(constraint),
                source.span(),
            )))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let table = self.resolve_with_names(&mut context, source, &mut names)?;
        let context = context.apply_create_table(&table)?;
        Ok(AstAndContextPair::new(
            SemAst::create_table(&table),
            context,
        ))
    }

    #[cfg(any(test, feature = "sqlite"))]
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<CreateTable, AnalysisError> {
        let mut names = self.constraint_names(context);
        self.resolve_with_names(context, source, &mut names)
    }

    pub(crate) fn resolve_with_names(
        &self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<CreateTable, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, source, names);
        context.finish_analysis(result)
    }

    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<CreateTable, AnalysisError> {
        if self.ast.query.is_some() {
            todo!()
        }
        let name = TableName::from(convert(&self.ast.name).full_name()?);
        let mut columns = Vec::new();
        let mut serials = serial::SerialSequences::default();
        for column in &self.ast.columns {
            columns.push(convert(column).resolve_serial(
                context,
                &name,
                self.ast.temporary,
                &mut serials,
            )?);
        }
        let mut table = CreateTable::new(&name, columns, self.ast.mode, self.ast.temporary, source);
        table.retain_serial_sequences(serials.into_sequences());
        for definition in &self.ast.constraints {
            if let Some(constraint) = convert(definition).resolve_declared_key(context, &table)? {
                table.record_declared_constraint(&constraint)?;
            }
        }
        for column in &self.ast.columns {
            for definition in &column.constraints {
                if definition.name.is_some() && definition.constraint.is_unique_key() {
                    let keys = [ColumnName::from(column.name.as_str())];
                    let constraint = convert(&definition.constraint).resolve_table(
                        context,
                        &name,
                        definition.name.as_deref().expect("named inline key"),
                        &keys,
                    )?;
                    table.record_declared_constraint(&constraint)?;
                }
            }
        }
        self.resolve_foreign_keys(context, &mut table)?;
        if context.dialect() == Dialect::postgres() {
            table.apply_primary_key_nullability();
        }
        self.resolve_inline_checks(context, &mut table, names)?;
        self.resolve_checks(context, &mut table, names)?;
        Ok(table)
    }
}

impl AstConversion<'_, Table> {
    pub(crate) fn resolve_foreign_keys(
        &self,
        context: &mut impl ParseContext,
        table: &mut CreateTable,
    ) -> Result<(), AnalysisError> {
        for definition in &self.ast.constraints {
            if let Some(constraint) =
                convert(definition).resolve_declared_foreign_key(context, table)?
            {
                table.record_declared_constraint(&constraint)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_create_table_foreign_keys {
    use crate::test_helpers::ownership::apply_ownership_sql;

    use crate::{context::TableInfoRead, names::ColumnName, test_helpers::foreign_keys as fk};

    #[test]
    fn create_result_and_catalog_retain_composite_and_unnamed_foreign_keys()
    -> testresult::TestResult {
        let context = fk::apply(
            fk::context(),
            "CREATE TABLE a.parent(x integer, y integer, PRIMARY KEY(x,y), UNIQUE(y,x))",
        );
        let pair = apply_ownership_sql(
            context,
            "CREATE TABLE a.child(x integer, y integer, CONSTRAINT fk FOREIGN KEY(y,x) REFERENCES parent(y,x), FOREIGN KEY(x,y) REFERENCES parent, FOREIGN KEY(y,x) REFERENCES parent(y,x))",
        )?;
        let result = pair.ast().as_create_table().unwrap();
        assert_eq!(result.foreign_keys().count(), 3);
        let constraint = result.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.source_columns(),
            &[ColumnName::from("y"), ColumnName::from("x")]
        );
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("y"), ColumnName::from("x")].as_slice())
        );
        let context = pair.into_context();
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        assert_eq!(child.foreign_keys().count(), 3);
        assert_eq!(
            child
                .foreign_keys()
                .filter(|key| key.name().is_empty())
                .count(),
            2
        );
        assert!(child.foreign_keys().any(|key| key.target_columns()
            == Some([ColumnName::from("x"), ColumnName::from("y")].as_slice())));
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(())
    }

    #[rstest::rstest]
    #[case::source("absent", "parent(id)", "column_not_found_in_table", true)]
    #[case::postgres_case_sensitive("\"ID\"", "parent(id)", "column_not_found_in_table", true)]
    #[case::target_column("id", "parent(absent)", "column_not_found_in_table", true)]
    #[case::target_relation("id", "missing(id)", "relation_not_found", false)]
    fn rejected_create_retains_analysis_without_publishing_table(
        #[case] source: &str,
        #[case] reference: &str,
        #[case] code: &str,
        #[case] bound: bool,
    ) -> testresult::TestResult {
        let context = fk::parents();
        let pair = apply_ownership_sql(
            context,
            &format!(
                "CREATE TABLE a.child(id integer, CONSTRAINT fk FOREIGN KEY({source}) REFERENCES {reference})"
            ),
        )?;
        let table = pair.ast().as_create_table().unwrap();
        let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert!(!constraint.references_can_apply());
        assert_eq!(
            constraint.target_table(),
            if bound {
                Some(fk::name("a", "parent"))
            } else {
                None
            }
            .as_ref()
        );
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(fk::name("a", "child").full_name())
                .is_none()
        );
        assert_eq!(context.problems().len(), 1);
        assert_eq!(context.problems()[0].code(), code);
        let context = fk::apply(context, "SELECT id FROM a.parent");
        assert_eq!(context.problems().len(), 1);
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::existing("Parent(id)", true)]
    #[case::forward("later(id)", false)]
    fn sqlite_create_table_foreign_keys_retain_declarations(
        #[case] reference: &str,
        #[case] resolved: bool,
    ) -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
        let sql = format!(
            "CREATE TABLE child(id, CONSTRAINT fk FOREIGN KEY(id) REFERENCES {reference} ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED)"
        );
        let context = fk::apply_sqlite(context, &sql);
        let child = context
            .get_table_impl(fk::name("main", "child").full_name())
            .unwrap();
        let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.target_table(),
            if resolved {
                Some(fk::name("main", "parent"))
            } else {
                None
            }
            .as_ref()
        );
        assert_eq!(
            ::serde_json::to_value(constraint.delete_action())?,
            "Cascade"
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch("CREATE TABLE Parent(id PRIMARY KEY)")?;
        oracle.execute_batch(&sql)?;
        let target: String = oracle.query_row(
            r#"SELECT "table" FROM pragma_foreign_key_list('child')"#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(target, if resolved { "Parent" } else { "later" });
        Ok(())
    }
}

impl AstConversion<'_, TableConstraint> {
    pub(crate) fn resolve_declared_foreign_key(
        &self,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Constraint>, AnalysisError> {
        let result = convert(&self.ast.constraint).resolve_declared_foreign_key(
            context,
            table,
            self.ast.name.as_deref().unwrap_or_default(),
            &self.column_names(),
        )?;
        if let Some(mut constraint) = result {
            let columns = self.delete_column_names();
            let valid = table.check_reference_columns(context, table.name(), &columns)?;
            constraint.restrict_reference_application(valid);
            constraint.set_foreign_key_delete_columns(&columns);
            Ok(Some(constraint))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn delete_column_names(&self) -> Vec<ColumnName> {
        self.ast
            .extras
            .delete_columns
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }

    pub(crate) fn column_names(&self) -> Vec<ColumnName> {
        self.ast
            .keys
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }
}

impl AstConversion<'_, AstColumnConstraint> {
    pub(crate) fn resolve_declared_foreign_key(
        &self,
        context: &mut impl ParseContext,
        table: &CreateTable,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Option<Constraint>, AnalysisError> {
        let AstColumnConstraint::ForeignKey(reference) = self.ast else {
            return Ok(None);
        };
        convert(reference)
            .resolve_table_in(context, table.name(), name, keys, Some(table))
            .map(Some)
    }
}

impl AstConversion<'_, AstColumn> {
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(ColumnDefinition, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_standalone(&mut context, table);
        let column = context.finish_analysis(result)?;
        Ok((column, context))
    }
    pub(crate) fn resolve_standalone(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<ColumnDefinition, AnalysisError> {
        let mut column = self.resolve(context, table)?;
        let canonical = context
            .resolve_table_name(table)
            .unwrap_or_else(|| table.clone());
        let mut names = ConstraintNames::default();
        self.reserve_constraint_names(context, &mut names);
        self.resolve_inline_checks(context, &canonical, None, &mut names, &mut column)?;
        Ok(column)
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<ColumnDefinition, AnalysisError> {
        let name = ColumnName::from(self.ast.name.as_str());
        let type_ref = self.resolve_type(context, table, &name)?;
        let mut column = ColumnDefinition::new(
            &Some(name.clone()),
            type_ref.as_ref(),
            self.ast.not_null,
            &[],
        );
        self.resolve_declaration(context, &mut column)?;
        self.resolve_constraints(context, table, &name, &mut column)?;
        Ok(column)
    }

    pub(crate) fn resolve_type(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let type_ref = if let Some(ty) = &self.ast.type_name {
            let type_name = convert(&ty.name).full_name()?;
            if let Some(ty) = context.get_type(&type_name) {
                if context.dialect() == Dialect::sqlite() {
                    Some(TypeReference::from_full_name(type_name))
                } else if let Some(type_ref) = ty.type_reference() {
                    let array = self
                        .type_name
                        .as_ref()
                        .is_some_and(|syntax| !syntax.array_bounds.is_empty());
                    Some(TypeReference::concrete_type_ref(
                        type_ref
                            .full_name()
                            .expect("catalog declaration has a nominal type identity"),
                        array,
                    ))
                } else {
                    let problem = AnalysisProblem::unexpected_dynamic_type(ty);
                    context.report_analysis_problem(problem)?;
                    None
                }
            } else {
                context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                    table, name, &type_name,
                ))?;
                None
            }
        } else {
            None
        };
        Ok(type_ref)
    }

    pub(crate) fn resolve_declaration(
        &self,
        context: &mut impl ParseContext,
        column: &mut ColumnDefinition,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == Dialect::postgres() {
            if let Some(type_name) = &self.ast.type_name {
                column.retain_requested_type(
                    &convert(type_name).declaration_type(column.get_type())?,
                );
            }
            let lookup = convert(&self.ast.collation)
                .optional_full_name()?
                .map(|name| context.lookup_collation(&name));
            column.validate_collation(context, lookup.as_ref())?;
        }
        Ok(())
    }

    pub(crate) fn resolve_constraints(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
        column: &mut ColumnDefinition,
    ) -> Result<(), AnalysisError> {
        for constraint in &self.ast.constraints {
            // CHECK needs the complete row, which is declared before expression analysis.
            if constraint.is_check() {
                continue;
            }
            // ENFORCED is the existing constraint behavior. Retain its explicit
            // syntax, but it does not add a separate semantic column constraint.
            if matches!(
                constraint.constraint,
                AstColumnConstraint::Enforcement(true)
            ) {
                continue;
            }
            if constraint.constraint.is_unique_key() && constraint.extras.deferrable {
                todo!("deferrable unique-key catalog representation");
            }
            let constraint = convert(&constraint.constraint).resolve(context, table, name)?;
            if let Some(nullability) = constraint.as_nullability() {
                column.set_not_null(Some(nullability.is_not_null()));
            }
            if context.dialect() == Dialect::sqlite() {
                if let Some(default) = constraint.as_default() {
                    column.set_default(default.expr());
                }
            } else if let Some(fk) = constraint.as_foreign_key()
                && let Some(col_type) = column.get_type()
                && let Some(pk_table) = fk.primary_key_table()
                && let Some(pk_type) = fk.primary_key_column_type()
                && col_type != pk_type
            {
                context.report_analysis_problem(
                    AnalysisProblem::foreign_key_column_type_mismatch(
                        table,
                        name,
                        col_type,
                        pk_table,
                        fk.primary_key_column(),
                        pk_type,
                    ),
                )?;
            }
            column.push_constraint(constraint);
        }
        Ok(())
    }
}

impl AstConversion<'_, AstColumnConstraint> {
    #[cfg(any(test, feature = "sqlite"))]
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(ColumnConstraint, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(&mut context, table, column);
        let constraint = context.finish_analysis(result)?;
        Ok((constraint, context))
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnConstraint, AnalysisError> {
        Ok(match self.ast {
            AstColumnConstraint::Enforcement(_) => {
                unreachable!("versioned enforcement is checked before resolution")
            }
            AstColumnConstraint::Nullability(not_null, conflict, validity) => {
                ColumnConstraint::nullability(ColumnNullabilityConstraint::from_ast_parts(
                    context, table, column, *not_null, conflict, validity,
                ))
            }
            AstColumnConstraint::Default(expression) => ColumnConstraint::default(
                DefaultColumnConstraint::from_ast_expression(context, table, column, expression)?,
            ),
            AstColumnConstraint::Check(_) => {
                self.resolve_standalone_check(context, table, column)?
            }
            AstColumnConstraint::PrimaryKey(conflict, deferrable) => {
                ColumnConstraint::primary_key(PrimaryKeyColumnConstraint::from_ast_parts(
                    context, table, column, conflict, deferrable,
                ))
            }
            AstColumnConstraint::Unique(conflict) => ColumnConstraint::unique(
                UniqueColumnConstraint::from_ast_conflict(context, table, column, conflict),
            ),
            AstColumnConstraint::Identity(mode) => ColumnConstraint::identity(
                IdentityColumnConstraint::from_ast_mode(context, table, column, mode),
            ),
            AstColumnConstraint::Generated(expression, mode) => {
                ColumnConstraint::generated(GeneratedColumnConstraint::from_ast_expression(
                    context, table, column, expression, mode,
                )?)
            }
            AstColumnConstraint::ForeignKey(reference) => {
                convert(reference).resolve(context, table, column)?
            }
            AstColumnConstraint::Exclusion => ColumnConstraint::exclusion(
                ExclusionColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Deferrability(value) => ColumnConstraint::attr_deferrability(
                AttrDeferrabilityColumnConstraint::from_ast_value(context, table, column, *value),
            ),
            AstColumnConstraint::Deferred => ColumnConstraint::attr_deferred(
                AttrDeferredColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Immediate => ColumnConstraint::attr_immediate(
                AttrImmediateColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Collate(name) => ColumnConstraint::collate(
                CollateColumnConstraint::from_ast_name(context, table, column, name)?,
            ),
        })
    }
}

impl AstConversion<'_, ForeignReference> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnConstraint, AnalysisError> {
        let target = TableName::from(convert(&self.ast.table).full_name()?);
        #[cfg(feature = "sqlite")]
        self.report_ignored_matches(context, table, column)?;
        let [target_column] = self.ast.columns.as_slice() else {
            return AnalysisError::raise_unexpected_input("foreign key target column count != 1");
        };
        let target_column = ColumnName::from(target_column.as_str());
        let (binding, target_type) =
            ForeignKeyTarget::resolve_column(context, &target, &target_column)?;
        Ok(ColumnConstraint::foreign_key(
            ForeignKeyColumnConstraint::new(
                table,
                column,
                binding.relation(),
                &target_column,
                target_type.as_ref(),
                self.ast.match_type.as_ref(),
                self.ast.delete_action.clone(),
                self.ast.update_action.clone(),
                self.ast.deferrability.clone(),
                self.ast.validity.clone(),
            ),
        ))
    }
}

impl AstConversion<'_, TableAs> {
    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        self.resolve_and_apply_at(context, cache, &source)
    }

    pub(crate) fn resolve_and_apply_at<TContext: ParseContext>(
        &self,
        mut context: TContext,
        cache: &mut CacheContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut query = convert(&self.ast.query).resolve(&mut context, cache)?;
        let name = convert(&self.ast.into.relation.name).full_name()?;
        if self.ast.into.columns.len() > query.result_columns().len() {
            context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                &name,
                "too many output column names",
                source.span(),
            )))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        query.rename_columns(&self.ast.into.columns);
        let columns = query.result_columns().column_definitions()?;
        let mut names = Vec::new();
        for column in &columns {
            if names.contains(&column.name()) {
                context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                    &name,
                    "duplicate output column name",
                    source.span(),
                )))?;
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            }
            names.push(column.name());
        }
        let table = CreateTable::new(
            &TableName::from(name.clone()),
            columns,
            CreateTableMode::from_if_not_exists(self.ast.if_not_exists),
            self.ast.into.relation.persistence == "t",
            source,
        );
        let table_as = CreateTableAs::new(&name, &SemAst::select(&query), self.ast.materialized)
            .with_if_not_exists(self.ast.if_not_exists);
        let context = if self.ast.materialized {
            context.apply_create_table_as(&table_as)?
        } else {
            context.apply_create_table(&table)?
        };
        Ok(AstAndContextPair::new(
            SemAst::create_table_as(&table_as),
            context,
        ))
    }
}

impl AstConversion<'_, Index> {
    #[cfg(test)]
    pub(crate) fn resolve_elements(
        elements: &[IndexExpression],
        context: &mut impl ParseContext,
    ) -> Result<IndexElementCollection, AnalysisError> {
        Self::resolve_elements_in(elements, context, &ScalarAnalysisContext::default())
    }

    pub(crate) fn resolve_elements_in(
        elements: &[IndexExpression],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<IndexElementCollection, AnalysisError> {
        let mut collection = IndexElementCollection::default();
        for element in elements {
            if element.using_order {
                return AnalysisError::raise_unexpected_input("SortbyUsing in IndexElem");
            }
            let value = if let Some(expr) = &element.expression {
                IndexElement::from(&convert(expr).resolve_in(context, scope)?)
            } else {
                IndexElement::from(&ColumnName::from(
                    element.name.as_deref().unwrap_or_default(),
                ))
            };
            let order = if element.descending == Some(true) {
                SortOrder::Desc
            } else {
                SortOrder::Asc
            };
            collection.push(IndexElementWithOrder::new(&value, order, &element.alias));
        }
        Ok(collection)
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<CreateIndex>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }

    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<CreateIndex>, AnalysisError> {
        let requested_table = TableName::from(convert(&self.ast.relation.name).full_name()?);
        let Some(table) = context.resolve_table_name(&requested_table) else {
            context
                .report_analysis_problem(AnalysisProblem::relation_not_found(&requested_table))?;
            return Ok(None);
        };
        let name = FullName::new(
            &table.full_name().catalog().cloned(),
            &table.full_name().schema().cloned(),
            &LocalName::from(self.ast.name.as_str()),
        );
        let alias = AliasSpec::from_local_name(&table.full_name().local_name());
        let with = WithClause::default();
        let mut from = FromClause::default();
        from.push(DataSource::resolve_relation(
            context,
            table.full_name(),
            &alias,
            &with,
        )?);
        let scope = ScalarAnalysisContext::nested(&with, &from, None);
        let keys = Self::resolve_elements_in(&self.ast.keys, context, &scope)?;
        let includes = Self::resolve_elements_in(&self.ast.includes, context, &scope)?;
        let predicate =
            AstConversion::<Expr>::resolve_optional(&self.ast.predicate, context, &scope)?;
        let mut index =
            CreateIndex::from_resolved(&name, &table, &keys, &includes, self.ast.unique);
        index = index.with_if_not_exists(self.ast.if_not_exists);
        index.set_predicate(predicate);
        if !self.initialize_index_storage(context, &mut index)? {
            return Ok(None);
        }
        Ok(Some(index))
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(index) = self.resolve(&mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = context.apply_create_index(&index)?;
        Ok(AstAndContextPair::new(
            SemAst::create_index(&index),
            context,
        ))
    }
}

impl AstConversion<'_, Schema> {
    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        context.apply_creation_group(|context| self.resolve_group(context, cache))
    }
    pub(crate) fn resolve_group<TContext: ParseContext>(
        &self,
        mut context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if context.schema_definition(&self.ast.name).is_some() {
            let schema =
                CreateSchema::new(&self.ast.name, &[]).with_if_not_exists(self.ast.if_not_exists);
            let context = context.apply_create_schema(&schema)?;
            return Ok(AstAndContextPair::new(
                SemAst::create_schema(&schema),
                context,
            ));
        }
        let owner = if let Some(authorization) = &self.ast.authorization {
            Some(convert(authorization).resolve_owner(&mut context)?)
        } else {
            None
        };
        let previous_path = context.get_search_path().to_vec();
        let mut schema_path = vec![crate::names::SchemaName::from(self.ast.name.as_str())];
        for schema in &previous_path {
            if !schema_path.contains(schema) {
                schema_path.push(schema.clone());
            }
        }
        context.set_search_path(&schema_path);
        let mut elements = Vec::new();
        for element in &self.ast.elements {
            let AstAndContextPair(element, next) =
                convert(element).resolve_and_apply(context, cache)?;
            context = next;
            elements.push(element);
        }
        context.set_search_path(&previous_path);
        let mut schema =
            CreateSchema::new(&self.ast.name, &elements).with_if_not_exists(self.ast.if_not_exists);
        if let Some(owner) = owner {
            schema.change_owner(&owner);
        }
        let context = context.apply_create_schema(&schema)?;
        Ok(AstAndContextPair::new(
            SemAst::create_schema(&schema),
            context,
        ))
    }
}

impl AstConversion<'_, FunctionParameter> {
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<(FunctionParam, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(&mut context);
        let parameter = context.finish_analysis(result)?;
        Ok((parameter, context))
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<FunctionParam, AnalysisError> {
        let name = Some(self.ast.name.clone());
        let type_name = convert(&self.ast.type_name.name).full_name()?;
        let param_type = context.resolve_type_name(&type_name).map(|name| {
            TypeReference::concrete_type_ref(&name, !self.ast.type_name.array_bounds.is_empty())
        });
        let default = self
            .default
            .as_ref()
            .map(|expr| convert(expr).resolve_in(context, &ScalarAnalysisContext::default()))
            .transpose()?;
        Ok(convert(&self.ast.mode).resolve_parameter(&name, &param_type, &default))
    }
}

impl AstConversion<'_, ParameterMode> {
    pub(crate) fn resolve_parameter(
        self,
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default: &Option<SemScalarExpr>,
    ) -> FunctionParam {
        match self.ast {
            ParameterMode::In => FunctionParam::new_input_param(name, param_type, default),
            ParameterMode::Out => FunctionParam::new_output_param(name, param_type),
            ParameterMode::InOut => FunctionParam::new_in_out_param(name, param_type, default),
            ParameterMode::Variadic => FunctionParam::new_variadig_param(name, param_type),
            ParameterMode::Table => FunctionParam::new_table_param(name, param_type),
        }
    }
}

impl AstConversion<'_, Function> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateFunction, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_function_null_input_semantics {
    use crate::sem::convert;
    use crate::{
        ast::{TryFromWithTokens, ddl::Function},
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext},
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::called_on_null("CALLED ON NULL INPUT", false)]
    #[case::returns_null("RETURNS NULL ON NULL INPUT", true)]
    #[case::strict("STRICT", true)]
    fn honors_the_explicit_null_input_choice(#[case] option: &str, #[case] expected: bool) {
        let syntax = SynBuilder::new().parse(&format!(
            "CREATE FUNCTION f() RETURNS integer LANGUAGE sql {option} AS 'SELECT 1'"
        ));
        let function = Function::try_from_with_tokens(
            &syntax.as_create_function_stmt().as_inner().unwrap(),
            &IVec::default(),
        )
        .unwrap();
        let mut context = TestParseContext::default();
        let resolved = convert(&function).resolve(&mut context).unwrap();
        assert_eq!(
            expected,
            resolved.overloads().get_at(0).unwrap().is_strict()
        );
    }
}

impl AstConversion<'_, Function> {
    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateFunction, AnalysisError> {
        let name = convert(&self.ast.name).full_name()?;
        if self.ast.procedure || self.ast.result_type.is_none() {
            let mut params = Vec::new();
            let mut outputs = Vec::new();
            for param in &self.ast.parameters {
                let resolved = convert(param).resolve(context)?;
                if param.mode.is_output() {
                    outputs.push(resolved.get_type().cloned());
                }
                params.push(resolved);
            }
            let ret_type = if self.ast.procedure {
                None
            } else if outputs.len() == 1 {
                outputs.pop().flatten()
            } else if !outputs.is_empty() {
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "record"),
                    false,
                ))
            } else {
                return AnalysisError::raise_unexpected_none("create_function.return_type");
            };
            let strict = self.ast.options.iter().any(FunctionOption::is_strict);
            let overload = OverloadVariant::new(&ret_type, &params, strict, false);
            return Ok(CreateFunction::new(
                &name,
                &FunctionOverloadCollection::new(&[overload]),
                &None,
            )
            .with_procedure_kind(self.ast.procedure));
        }
        let Some(result_type) = &self.ast.result_type else {
            return AnalysisError::raise_unexpected_none("create_function.return_type");
        };
        let ret_name = convert(&result_type.name).full_name()?;
        let returning = if result_type.set_of {
            if ret_name.name().as_str() == "record" {
                let mut columns = Vec::new();
                for param in &self.ast.parameters {
                    if param.mode.is_output() {
                        let ty = TypeReference::from_full_name(
                            convert(&param.type_name.name).full_name()?,
                        );
                        columns.push(ColumnDefinition::new(
                            &Some(ColumnName::from(param.name.as_str())),
                            Some(&ty),
                            None,
                            &[],
                        ));
                    }
                }
                Some(ColumnCollection::new(columns))
            } else if let Some(ty) = context.get_type(&ret_name) {
                if ty.is_composite_type() {
                    Some(ty.get_composite_type_columns())
                } else {
                    ty.type_reference().map(|ty| {
                        ColumnCollection::new(vec![ColumnDefinition::new(
                            &None,
                            Some(ty),
                            None,
                            &[],
                        )])
                    })
                }
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::function_result_set_type_not_found(&ret_name),
                )?;
                Some(ColumnCollection::new(Vec::new()))
            }
        } else {
            None
        };
        let mut params = Vec::new();
        for param in &self.ast.parameters {
            params.push(convert(param).resolve(context)?);
        }
        let ret_type = if let Some(ty) = context.get_type(&ret_name) {
            ty.type_reference().cloned()
        } else {
            None
        };
        let strict = self.ast.options.iter().any(FunctionOption::is_strict);
        let overload = OverloadVariant::new(&ret_type, &params, strict, false);
        Ok(CreateFunction::new(
            &name,
            &FunctionOverloadCollection::new(&[overload]),
            &returning,
        ))
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let function = self.resolve(&mut context)?.with_replace(self.ast.replace);
        let preserves_rows = self
            .result_type
            .as_ref()
            .is_none_or(|result| !result.set_of)
            && !self
                .options
                .iter()
                .any(|option| matches!(option, FunctionOption::Window));
        let context = context
            .apply_create_function(&function)?
            .apply_function_view_rows(&function, preserves_rows)?;
        Ok(AstAndContextPair::new(
            SemAst::create_function(&function),
            context,
        ))
    }
}

impl AstConversion<'_, CompositeType> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateCompositeType, AnalysisError> {
        let name = convert(&self.ast.name).full_name()?;
        let table = TableName::from(name.clone());
        let mut fields = CompositeFieldCollection::default();
        for field in &self.ast.fields {
            fields.push(convert(field).resolve(context, &table)?);
        }
        Ok(CreateCompositeType::new(&name, &fields))
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let composite = self.resolve(&mut context)?;
        let context = context.apply_create_comp_type(&composite)?;
        Ok(AstAndContextPair::new(
            SemAst::create_composite_type(&composite),
            context,
        ))
    }
}

impl AstConversion<'_, CompositeField> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        composite: &TableName,
    ) -> Result<CompositeFieldDefinition, AnalysisError> {
        let requested = convert(&self.ast.type_name).reference()?;
        let type_ref = context.canonicalize_type_ref(&requested);
        if type_ref.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                composite,
                &ColumnName::from(self.ast.name.as_str()),
                requested
                    .full_name()
                    .expect("SQL type syntax constructs a concrete reference"),
            ))?;
        }
        let mut field =
            CompositeFieldDefinition::new(&self.ast.name, &type_ref, &Some(self.ast.is_not_null));
        field.retain_requested_type(
            &convert(&self.ast.type_name).declaration_type(type_ref.as_ref())?,
        );
        let lookup = self
            .collation
            .as_ref()
            .map(|syntax| convert::<Name>(syntax).full_name())
            .transpose()?
            .map(|name| context.lookup_collation(&name));
        field.validate_collation(context, lookup.as_ref())?;
        Ok(field)
    }
}

#[cfg(test)]
mod test_composite_field_resolution {
    use crate::names::{FullName, TableName};
    use crate::sem::convert;

    use crate::{
        ast::{TryFromWithTokens, ddl::CompositeType},
        sem::{PgBuiltInType, TypeReference},
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext},
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::array("integer[]", true)]
    #[case::dimensions("integer[][]", true)]
    #[case::bound("integer[3]", true)]
    #[case::scalar("integer", false)]
    fn retains_declared_array_identity(#[case] declaration: &str, #[case] is_array: bool) {
        let syntax = SynBuilder::new().parse(&format!(
            "CREATE TYPE public.array_value AS (items {declaration})"
        ));
        let composite = CompositeType::try_from_with_tokens(
            &syntax.as_composite_type_stmt().as_inner().unwrap(),
            &IVec::default(),
        )
        .unwrap();
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        let field = convert(&composite.fields[0])
            .resolve(
                &mut context,
                &TableName::from(FullName::with_schema("public", "array_value")),
            )
            .unwrap();
        assert_eq!(field.name(), "items");
        assert_eq!(
            field.get_type(),
            Some(TypeReference::concrete_type_ref(
                PgBuiltInType::int4()
                    .full_name()
                    .expect("catalog declaration has a nominal type identity"),
                is_array,
            ))
        );
    }
}

impl AstConversion<'_, Domain> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateDomain, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod domain_tests {
    use crate::sem::convert;
    use crate::test_helpers::create_cache_context;

    use crate::{
        ast::{StatementNode, TryFromWithTokens},
        sem::SemAst,
        test_helpers::{SynBuilder, TestParseContext},
    };
    use ::sql_fun_core::IVec;

    #[test]
    fn missing_base_type_is_diagnosed_only_during_resolution() -> testresult::TestResult {
        let parser = SynBuilder::new();
        let syntax = StatementNode::try_from_with_tokens(
            &parser.parse("create domain email as absent"),
            &IVec::default(),
        )?;
        let mut context = TestParseContext::default();
        let domain = convert(&syntax).resolve(&mut context, &mut create_cache_context())?;
        assert!(matches!(domain, SemAst::CreateDomain(_)));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl AstConversion<'_, Domain> {
    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateDomain, AnalysisError> {
        let name = convert(&self.ast.name).full_name()?;
        let base_name = convert(&self.ast.base.name).full_name()?;
        let base = context.canonicalize_type_ref(&convert(&self.ast.base).reference()?);
        if base.is_none() {
            context.report_analysis_problem(AnalysisProblem::domain_base_type_not_found(
                &name,
                &base_name.to_string(),
                &self.ast.base_span,
            ))?;
        }
        let mut domain = CreateDomain::new(&TypeReference::from_full_name(name), &base);
        let category = if let Some(base) = context.get_type(&base_name) {
            if self.ast.base.array_bounds.is_empty() {
                base.category(context)
            } else if context.dialect() == Dialect::postgres() {
                TypeCategory::try_from('A').ok()
            } else {
                None
            }
        } else {
            None
        };
        domain.set_category(category);
        domain
            .retain_requested_type(&convert(&self.ast.base).declaration_type(domain.base_type())?);
        let lookup = convert(&self.ast.collation)
            .optional_full_name()?
            .map(|name| context.lookup_collation(&name));
        domain.validate_collation(context, lookup.as_ref())?;
        Ok(domain)
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !self.check_engine_features(&mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let domain = self.resolve(&mut context)?;
        let context = context.apply_create_domain(&domain)?;
        Ok(AstAndContextPair::new(
            SemAst::create_domain(&domain),
            context,
        ))
    }
}

impl AstConversion<'_, AlterRelation> {
    pub(crate) fn view_default_changes(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterView>, ResolutionFailure> {
        let requested = convert(&self.ast.name).full_name()?;
        let Some(name) = expand_relation_search_path(context, &requested, |key| {
            context
                .get_view_impl(key)
                .filter(|view| !view.is_materialized())
                .map(|_| key.clone())
        }) else {
            return Ok(None);
        };
        let mut actions = Vec::new();
        for command in &self.ast.commands {
            let Some(action) = convert(command).view_default_action(context)? else {
                return Ok(None);
            };
            actions.push(action);
        }
        let change = AlterView::new(&ViewName::from_full_name(&name), &actions);
        let mut view = context
            .get_view_impl(&name)
            .expect("resolved ordinary View")
            .clone();
        if !view.apply_alter(context, &change)? {
            return Err(ResolutionFailure::reported());
        }
        Ok(Some(change))
    }
}

impl AstConversion<'_, AlterCommand> {
    pub(crate) fn view_default_action(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterObjSubCommand>, ResolutionFailure> {
        match self.ast {
            AlterCommand::Postgres(command) => {
                convert::<PostgresAlterCommand>(command).view_default_action(context)
            }
            AlterCommand::Default(name, expression) => {
                AlterObjSubCommand::from_ast_view_default(context, name, expression).map(Some)
            }
            _ => Ok(None),
        }
    }
}

impl AstConversion<'_, PostgresAlterCommand> {
    pub(crate) fn view_default_action(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterObjSubCommand>, ResolutionFailure> {
        match self.ast.operation {
            AlterOperation::ColumnDefault => self.view_column_default(context),
            AlterOperation::EnableTrigger
            | AlterOperation::EnableAlwaysTrigger
            | AlterOperation::EnableTriggerAll
            | AlterOperation::EnableTriggerUser
            | AlterOperation::DisableTrigger
            | AlterOperation::EnableReplicaTrigger
            | AlterOperation::DisableTriggerAll
            | AlterOperation::DisableTriggerUser
            | AlterOperation::EnableRule
            | AlterOperation::EnableAlwaysRule
            | AlterOperation::DisableRule
            | AlterOperation::EnableReplicaRule => {
                Err(ResolutionFailure::view(&ViewProblem::invalid_alter()))
            }
            _ => Ok(None),
        }
    }

    fn view_column_default(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterObjSubCommand>, ResolutionFailure> {
        let value = convert(&self.ast.definition).view_default(context)?;
        let column = ColumnName::from(self.ast.name.as_str());
        let action = match value {
            Some(value) => AlterObjSubCommand::column_default(&column, &value),
            None => AlterObjSubCommand::drop_column_default(&column),
        };
        Ok(Some(action))
    }
}

impl AstConversion<'_, AlterDefinition> {
    pub(crate) fn view_default(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        match self.ast {
            AlterDefinition::None => Ok(None),
            AlterDefinition::Expression(value) => convert(value).resolved_view_default(context),
            _ => {
                AnalysisError::raise_unexpected_input("view default expression").map_err(Into::into)
            }
        }
    }
}

impl AlterObjSubCommand {
    fn from_ast_view_default(
        context: &mut impl ParseContext,
        name: &str,
        expression: &Expr,
    ) -> Result<Self, ResolutionFailure> {
        let value = convert(expression).resolve(
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )?;
        Ok(Self::column_default(&ColumnName::from(name), &value))
    }
}

impl AstConversion<'_, Expr> {
    fn resolved_view_default(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        self.resolve(
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )
        .map(Some)
        .map_err(Into::into)
    }
}

impl AstConversion<'_, AlterCommand> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match self.ast {
            AlterCommand::Postgres(command) => {
                convert(&command.definition).check_engine_features(context)
            }
            AlterCommand::Constraint(constraint) => {
                convert(constraint).check_engine_features(context, false)
            }
            _ => Ok(true),
        }
    }
}

impl AstConversion<'_, AlterDefinition> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match self.ast {
            AlterDefinition::Constraint(constraint) => {
                convert(constraint).check_engine_features(context, false)
            }
            AlterDefinition::Column(column) => convert(column).check_engine_features(context),
            AlterDefinition::ConstraintFlags(flags) => {
                convert(flags).check_engine_features(context)
            }
            _ => Ok(true),
        }
    }
}

impl AstConversion<'_, AlterConstraintFlags> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        use crate::engine_specific::{Postgres18Feature, Postgres18Syntax};
        let mut valid = true;
        if self.ast.enforced.is_some() {
            valid &= context.postgres18_syntax_used(Postgres18Syntax::ConstraintEnforcement)?;
        }
        if self.ast.no_inherit.is_some() {
            valid &= context.postgres18_syntax_used(Postgres18Syntax::ConstraintInheritance)?;
        }
        if valid && (self.ast.enforced.is_some() || self.ast.no_inherit.is_some()) {
            todo!("ALTER CONSTRAINT enforcement/inheritance (#887)");
        }
        Ok(valid)
    }
}

impl AstConversion<'_, Domain> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for constraint in &self.ast.constraints {
            valid &= convert(constraint).check_engine_features(context, true)?;
        }
        if valid
            && self.ast.constraints.iter().any(|constraint| {
                matches!(constraint.constraint, AstColumnConstraint::Enforcement(_))
            })
        {
            todo!("domain constraint analysis (#548)");
        }
        Ok(valid)
    }
}
