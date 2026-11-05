//! Destination-owned conversion of common DDL syntax and catalog changes.

mod alter_index;
mod check;
mod column_attributes;
mod column_constraint;
mod column_rename;
mod serial;
mod table_rename;

use self::serial::SerialSequences;
use crate::sem::OwnerSpecification;
use crate::sem::SelectStatement;
use crate::sem::WithTypmod;
use crate::sem::{CheckConstraint, ColumnAttributeAction};
use crate::sem::{EngineFeatureValidation, IndexAlteration};

use crate::ast::ddl::{
    AlterConstraintFlags, AlterDefinition, CompositeField, CompositeType, Domain, Function,
    FunctionParameter, Index, ParameterMode, Schema, TableAs,
};
use crate::ast::ddl::{AlterOperation, FunctionOption, PostgresAlterCommand};
use crate::ast::{
    AlterRelation, AlterRelationKind, ColumnConstraint as AstColumnConstraint, ForeignReference,
    Table,
};

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

/// Coordinates ALTER semantics and catalog application across relation kinds.
pub(crate) struct RelationAlteration;

impl RelationAlteration {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AlterRelation,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let source = context.get_source_path_with_span().cloned();
        Self::from_ast_and_apply_with_source(ast, context, source.as_ref())
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &AlterRelation,
        context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        Self::from_ast_and_apply_with_source(ast, context, Some(source))
    }

    pub(crate) fn from_ast_and_apply_with_source<TContext: ParseContext>(
        ast: &AlterRelation,
        mut context: TContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::alter_relation(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let changes = AlterView::from_ast_defaults(ast, &mut context);
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
        let result = Self::from_ast_inner(ast, &mut context, source);
        let statement = context.finish_analysis(result)?;
        let context = statement.apply_relation_alter(context)?;
        Ok(AstAndContextPair::new(statement, context))
    }

    pub(crate) fn from_ast_inner(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        if ast.kind.supports_table_column_attributes() && Self::has_ast_column_rename(ast) {
            return Self::from_ast_column_rename(ast, context, source);
        }
        if ast.kind.supports_table_column_attributes() && Self::has_ast_table_rename(ast) {
            return Self::from_ast_table_rename(ast, context, source);
        }
        if ast.kind.supports_table_column_attributes() && Self::has_ast_column_attributes(ast) {
            return Self::from_ast_column_attributes(ast, context, source);
        }
        Self::from_ast_kind(
            &ast.kind,
            context,
            &FullName::from_ast_name(&ast.name)?,
            &ast.commands,
            source,
            ast.if_exists,
        )
    }

    pub(crate) fn from_ast_kind(
        ast: &AlterRelationKind,
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        source: Option<&SourcePathWithSpan>,
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        match ast {
            AlterRelationKind::Table => {
                Self::table_from_ast_commands(context, name, syntax_commands, source)
            }
            AlterRelationKind::View => {
                Self::view_from_ast_commands(context, name, syntax_commands, false)
            }
            AlterRelationKind::MaterializedView => {
                Self::view_from_ast_commands(context, name, syntax_commands, true)
            }
            AlterRelationKind::Sequence => {
                Self::sequence_from_ast_commands(context, name, syntax_commands)
            }
            AlterRelationKind::Index => {
                Self::index_from_ast_commands(context, name, syntax_commands, if_exists)
            }
            AlterRelationKind::ForeignTable | AlterRelationKind::CompositeType => todo!(),
        }
    }

    pub(crate) fn index_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        let mut operations = Vec::new();
        for command in syntax_commands {
            operations.push(IndexAlteration::from_ast_command(command, context)?);
        }
        let statement = AlterIndex::resolve(context, name, if_exists, &operations)?;
        Ok(SemAst::alter_index(&statement))
    }

    pub(crate) fn table_from_ast_commands(
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
            commands.push(AlterObjSubCommand::from_ast(command, context, &table)?);
        }
        let mut statement = AlterTable::new(&table, &commands, source);
        statement.validate_check_names(context)?;
        Ok(SemAst::alter_table(&statement))
    }

    pub(crate) fn view_from_ast_commands(
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
            .map(|command| AlterObjSubCommand::from_ast_owner(command, context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_view(&AlterView::new(&view, &commands)))
    }

    pub(crate) fn sequence_from_ast_commands(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[AlterCommand],
    ) -> Result<SemAst, AnalysisError> {
        if context.get_sequence(name).is_none() {
            context.report_analysis_problem(AnalysisProblem::sequence_not_found(name))?;
        }
        let commands = syntax_commands
            .iter()
            .map(|command| AlterObjSubCommand::from_ast_owner(command, context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_sequence(&AlterSequence::new(name, &commands)))
    }
}

impl AlterObjSubCommand {
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AlterCommand,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context, table);
        let command = context.finish_analysis(result)?;
        Ok((command, context))
    }

    pub(crate) fn from_ast_owner(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AlterCommand::Postgres(command) => Self::from_ast_postgres_owner(command, context),
            AlterCommand::Owner(owner) => Ok(Self::resolved_owner(&OwnerSpecification::from_ast(
                owner, context,
            )?)),
            _ => todo!(),
        }
    }

    pub(crate) fn check_ast_missing_column(
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

    pub(crate) fn check_ast_existing_column(
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

    pub(crate) fn from_ast(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        match ast {
            AlterCommand::Postgres(command) => Self::from_ast_postgres(command, context, table),
            AlterCommand::Owner(_) => Self::from_ast_owner(ast, context),
            AlterCommand::Default(name, expr) => Self::from_ast_default(context, table, name, expr),
            AlterCommand::Constraint(constraint) => {
                Self::from_ast_constraint(context, table, constraint)
            }
            AlterCommand::Cluster(name) => Self::from_ast_cluster(context, table, name),
            AlterCommand::Rename(name) => Self::from_ast_rename(context, name),
            AlterCommand::AddColumn(column) => Self::from_ast_add_column(context, table, column),
            AlterCommand::AlterColumn(name, column) if context.dialect() == Dialect::sqlite() => {
                Self::from_ast_alter_column(context, table, name, column)
            }
            AlterCommand::RenameColumn(from, to) => {
                Self::from_ast_rename_column(context, table, from, to)
            }
            AlterCommand::DropColumn(name) if context.dialect() == Dialect::sqlite() => {
                Self::from_ast_drop_column(context, table, name)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn from_ast_default(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        expr: &Expr,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        let expr = SemScalarExpr::from_ast_in(expr, context, &ScalarAnalysisContext::default())?;
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::column_default(&name, &expr))
    }

    pub(crate) fn from_ast_constraint(
        context: &mut impl ParseContext,
        table: &TableName,
        constraint: &TableConstraint,
    ) -> Result<Self, AnalysisError> {
        // Constraint-created indexes need the matched key even when the stored
        // table declaration remains unqualified.
        let resolved_table = context.resolve_table_name(table);
        let table = resolved_table.as_ref().unwrap_or(table);
        Ok(Self::add_constraint(&Constraint::from_ast_table(
            constraint, context, table,
        )?))
    }

    pub(crate) fn from_ast_cluster(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        let Some(resolved_table) = context.resolve_table_name(table) else {
            // The enclosing ALTER TABLE resolver already reports the absent table.
            return Ok(Self::cluster_on(name));
        };
        let index_name = FullName::new(
            &resolved_table.full_name().catalog().cloned(),
            &resolved_table.full_name().schema().cloned(),
            &LocalName::from(name),
        );
        if context.get_index(&index_name).is_none() {
            context.report_analysis_problem(AnalysisProblem::index_not_found(name))?;
        }
        Ok(Self::cluster_on(name))
    }

    pub(crate) fn from_ast_rename(
        context: &mut impl ParseContext,
        name: &Name,
    ) -> Result<Self, AnalysisError> {
        let name = TableName::from(FullName::from_ast_name(name)?);
        if context.get_table(&name).is_some() {
            context.report_analysis_problem(AnalysisProblem::relation_already_exists(&name))?;
        }
        Ok(Self::rename_to(&name))
    }

    pub(crate) fn from_ast_add_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        let column = ColumnDefinition::from_ast_standalone(column, context, table)?;
        if let Some(name) = column.name() {
            Self::check_ast_existing_column(context, table, name)?;
        }
        Ok(Self::add_column(&column))
    }

    pub(crate) fn from_ast_alter_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        column: &AstColumn,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::alter_column(
            &name,
            &ColumnDefinition::from_ast(column, context, table)?,
        ))
    }

    pub(crate) fn from_ast_rename_column(
        context: &mut impl ParseContext,
        table: &TableName,
        from: &str,
        to: &str,
    ) -> Result<Self, AnalysisError> {
        let (_, rename) = ColumnRename::resolve(
            context,
            table,
            &ColumnName::from(from),
            &ColumnName::from(to),
            false,
        )?;
        Ok(Self::analyzed_column_rename(&rename))
    }

    pub(crate) fn from_ast_drop_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(name);
        Self::check_ast_missing_column(context, table, &name)?;
        Ok(Self::drop_column(&name))
    }

    pub(crate) fn from_ast_postgres_owner(
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        if let Some(legacy) = &ast.legacy {
            Self::from_ast_owner(legacy, context)
        } else {
            todo!()
        }
    }

    pub(crate) fn from_ast_postgres(
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ColumnAttributeAction::phase_from_ast_operation(&ast.operation).is_some() {
            return Self::from_ast_single_column_attribute(ast, context, table);
        }
        if let Some(legacy) = &ast.legacy {
            Self::from_ast(legacy, context, table)
        } else {
            todo!()
        }
    }
}

impl Constraint {
    pub(crate) fn from_ast_declared_key(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Self>, AnalysisError> {
        if !ast.constraint.is_unique_key() {
            return Ok(None);
        }
        if ast.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let keys = Self::columns_from_ast(ast);
        table.check_reference_columns(context, table.name(), &keys)?;
        Self::from_ast_column_table(
            &ast.constraint,
            context,
            table.name(),
            ast.name.as_deref().unwrap_or_default(),
            &keys,
        )
        .map(Some)
    }

    pub(crate) fn from_ast_table(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ast.is_check() {
            let definition = context.get_table(table).cloned();
            return CheckConstraint::from_ast_table_constraint(
                ast,
                context,
                table,
                definition.as_ref(),
                true,
            )
            .map(Constraint::Check);
        }
        let keys = Self::columns_from_ast(ast);
        if context.get_table(table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(table))?;
        }
        for key in &keys {
            AlterObjSubCommand::check_ast_missing_column(context, table, key)?;
        }
        if ast.constraint.is_unique_key() && ast.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let name = ast.name.as_deref().unwrap_or_default();
        let mut constraint =
            Self::from_ast_column_table(&ast.constraint, context, table, name, &keys)?;
        constraint.set_foreign_key_delete_columns(&Self::delete_columns_from_ast(ast));
        Ok(constraint)
    }
}

impl ConstraintNames {
    pub(crate) fn ast_constraint_has_catalog_name(
        ast: &AstColumnConstraint,
        context: &impl ParseContext,
    ) -> bool {
        match ast {
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

impl Constraint {
    pub(crate) fn from_ast_column_table(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Self, AnalysisError> {
        match ast {
            AstColumnConstraint::PrimaryKey(_, _) => {
                Ok(Self::ast_table_primary_key(table, name, keys))
            }
            AstColumnConstraint::Unique(_) => Ok(Self::ast_table_unique(table, name, keys)),
            AstColumnConstraint::ForeignKey(reference) => {
                Self::from_ast_foreign_reference(reference, context, table, name, keys)
            }
            _ => todo!(),
        }
    }

    pub(crate) fn ast_table_primary_key(
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Self {
        Self::primary_key(&PrimaryKeyConstraint::new(name, table, keys))
    }

    pub(crate) fn ast_table_unique(table: &TableName, name: &str, keys: &[ColumnName]) -> Self {
        Self::unique(&UniqueConstraint::new(name, table, keys))
    }

    pub(crate) fn from_ast_foreign_reference(
        ast: &ForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_foreign_reference_in(ast, context, table, name, keys, None)
    }

    pub(crate) fn from_ast_foreign_reference_in(
        ast: &ForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
        pending: Option<&CreateTable>,
    ) -> Result<Self, AnalysisError> {
        #[cfg(feature = "sqlite")]
        for column in keys {
            Self::report_ast_ignored_matches(ast, context, table, column)?;
        }
        let requested = TableName::from(FullName::from_ast_name(&ast.table)?);
        let explicit = Self::referenced_columns_from_ast(ast);
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
                    ast.match_type.as_ref(),
                    ast.delete_action.as_ref(),
                    ast.update_action.as_ref(),
                    ast.deferrability.as_ref(),
                    ast.validity.as_ref(),
                );
        constraint.restrict_reference_application(source_valid && target_valid);
        Ok(Self::foreign(&constraint))
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn report_ast_ignored_matches(
        ast: &ForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        for name in &ast.ignored_matches {
            context.report_analysis_problem(
                AnalysisProblem::sqlite_ignores_match_option_in_fk_constraint(table, column, name),
            )?;
        }
        Ok(())
    }

    pub(crate) fn referenced_columns_from_ast(ast: &ForeignReference) -> Option<Vec<ColumnName>> {
        if ast.columns.is_empty() {
            None
        } else {
            Some(
                ast.columns
                    .iter()
                    .map(|key| ColumnName::from(key.as_str()))
                    .collect(),
            )
        }
    }
}

impl CreateTable {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Table,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        Self::from_ast_and_apply_at(ast, context, &source)
    }

    pub(crate) fn constraint_names_from_ast(
        ast: &Table,
        context: &impl ParseContext,
    ) -> ConstraintNames {
        let mut names = ConstraintNames::default();
        for definition in &ast.constraints {
            if !definition.is_check() {
                names.reserve(definition.name.as_deref().unwrap_or_default());
            }
        }
        for column in &ast.columns {
            ConstraintNames::reserve_ast_column(column, context, &mut names);
        }
        names
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &Table,
        mut context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::table(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let name = context.creation_name(&FullName::from_ast_name(&ast.name)?, ast.temporary)?;
        if !context.permits_relation_creation(
            &name,
            Kind::table(),
            ast.mode.if_not_exists(),
            false,
        )? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let mut names = Self::constraint_names_from_ast(ast, &context);
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
        let table = Self::from_ast_with_names(ast, &mut context, source, &mut names)?;
        let context = context.apply_create_table(&table)?;
        Ok(AstAndContextPair::new(
            SemAst::create_table(&table),
            context,
        ))
    }

    #[cfg(any(test, feature = "sqlite"))]
    pub(crate) fn from_ast(
        ast: &Table,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<Self, AnalysisError> {
        let mut names = Self::constraint_names_from_ast(ast, context);
        Self::from_ast_with_names(ast, context, source, &mut names)
    }

    pub(crate) fn from_ast_with_names(
        ast: &Table,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context, source, names);
        context.finish_analysis(result)
    }

    pub(crate) fn from_ast_inner(
        ast: &Table,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<Self, AnalysisError> {
        if ast.query.is_some() {
            todo!()
        }
        let name = TableName::from(FullName::from_ast_name(&ast.name)?);
        let mut columns = Vec::new();
        let mut serials = SerialSequences::default();
        for column in &ast.columns {
            columns.push(ColumnDefinition::from_ast_serial(
                column,
                context,
                &name,
                ast.temporary,
                &mut serials,
            )?);
        }
        let mut table = Self::new(&name, columns, ast.mode, ast.temporary, source);
        table.retain_serial_sequences(serials.into_sequences());
        for definition in &ast.constraints {
            if let Some(constraint) =
                Constraint::from_ast_declared_key(definition, context, &table)?
            {
                table.record_declared_constraint(&constraint)?;
            }
        }
        for column in &ast.columns {
            for definition in &column.constraints {
                if definition.name.is_some() && definition.constraint.is_unique_key() {
                    let keys = [ColumnName::from(column.name.as_str())];
                    let constraint = Constraint::from_ast_column_table(
                        &definition.constraint,
                        context,
                        &name,
                        definition.name.as_deref().expect("named inline key"),
                        &keys,
                    )?;
                    table.record_declared_constraint(&constraint)?;
                }
            }
        }
        Self::resolve_ast_foreign_keys(ast, context, &mut table)?;
        if context.dialect() == Dialect::postgres() {
            table.apply_primary_key_nullability();
        }
        Self::resolve_ast_inline_checks(ast, context, &mut table, names)?;
        Self::resolve_ast_checks(ast, context, &mut table, names)?;
        Ok(table)
    }

    pub(crate) fn resolve_ast_foreign_keys(
        ast: &Table,
        context: &mut impl ParseContext,
        table: &mut Self,
    ) -> Result<(), AnalysisError> {
        for definition in &ast.constraints {
            if let Some(constraint) =
                Constraint::from_ast_declared_foreign_key(definition, context, table)?
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

impl Constraint {
    pub(crate) fn from_ast_declared_foreign_key(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_column_foreign_key(
            &ast.constraint,
            context,
            table,
            ast.name.as_deref().unwrap_or_default(),
            &Self::columns_from_ast(ast),
        )?;
        if let Some(mut constraint) = result {
            let columns = Self::delete_columns_from_ast(ast);
            let valid = table.check_reference_columns(context, table.name(), &columns)?;
            constraint.restrict_reference_application(valid);
            constraint.set_foreign_key_delete_columns(&columns);
            Ok(Some(constraint))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn delete_columns_from_ast(ast: &TableConstraint) -> Vec<ColumnName> {
        ast.extras
            .delete_columns
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }

    pub(crate) fn columns_from_ast(ast: &TableConstraint) -> Vec<ColumnName> {
        ast.keys
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect()
    }

    pub(crate) fn from_ast_column_foreign_key(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &CreateTable,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Option<Self>, AnalysisError> {
        let AstColumnConstraint::ForeignKey(reference) = ast else {
            return Ok(None);
        };
        Self::from_ast_foreign_reference_in(
            reference,
            context,
            table.name(),
            name,
            keys,
            Some(table),
        )
        .map(Some)
    }
}

impl ColumnDefinition {
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstColumn,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_standalone(ast, &mut context, table);
        let column = context.finish_analysis(result)?;
        Ok((column, context))
    }

    pub(crate) fn from_ast_standalone(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        let mut column = Self::from_ast(ast, context, table)?;
        let canonical = context
            .resolve_table_name(table)
            .unwrap_or_else(|| table.clone());
        let mut names = ConstraintNames::default();
        ConstraintNames::reserve_ast_column(ast, context, &mut names);
        Self::resolve_ast_inline_checks(ast, context, &canonical, None, &mut names, &mut column)?;
        Ok(column)
    }

    pub(crate) fn from_ast(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(ast.name.as_str());
        let type_ref = Self::type_from_ast(ast, context, table, &name)?;
        let mut column = Self::new(&Some(name.clone()), type_ref.as_ref(), ast.not_null, &[]);
        Self::retain_ast_declaration(ast, context, &mut column)?;
        Self::resolve_ast_constraints(ast, context, table, &name, &mut column)?;
        Ok(column)
    }

    pub(crate) fn type_from_ast(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let type_ref = if let Some(ty) = &ast.type_name {
            let type_name = FullName::from_ast_name(&ty.name)?;
            if let Some(ty) = context.get_type(&type_name) {
                if context.dialect() == Dialect::sqlite() {
                    Some(TypeReference::from_full_name(type_name))
                } else if let Some(type_ref) = ty.type_reference() {
                    let array = ast
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

    pub(crate) fn retain_ast_declaration(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == Dialect::postgres() {
            if let Some(type_name) = &ast.type_name {
                column.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
                    type_name,
                    column.get_type(),
                )?);
            }
            let lookup = FullName::from_optional_ast_name(&ast.collation)?
                .map(|name| context.lookup_collation(&name));
            column.validate_collation(context, lookup.as_ref())?;
        }
        Ok(())
    }

    pub(crate) fn resolve_ast_constraints(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        for constraint in &ast.constraints {
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
            let constraint =
                ColumnConstraint::from_ast(&constraint.constraint, context, table, name)?;
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

impl ColumnConstraint {
    #[cfg(any(test, feature = "sqlite"))]
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstColumnConstraint,
        mut context: TContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context, table, column);
        let constraint = context.finish_analysis(result)?;
        Ok((constraint, context))
    }

    pub(crate) fn from_ast(
        ast: &AstColumnConstraint,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        Ok(match ast {
            AstColumnConstraint::Enforcement(_) => {
                unreachable!("versioned enforcement is checked before resolution")
            }
            AstColumnConstraint::Nullability(not_null, conflict, validity) => {
                Self::nullability(ColumnNullabilityConstraint::from_ast_parts(
                    context, table, column, *not_null, conflict, validity,
                ))
            }
            AstColumnConstraint::Default(expression) => Self::default(
                DefaultColumnConstraint::from_ast_expression(context, table, column, expression)?,
            ),
            AstColumnConstraint::Check(_) => {
                Self::from_ast_standalone_check(ast, context, table, column)?
            }
            AstColumnConstraint::PrimaryKey(conflict, deferrable) => {
                Self::primary_key(PrimaryKeyColumnConstraint::from_ast_parts(
                    context, table, column, conflict, deferrable,
                ))
            }
            AstColumnConstraint::Unique(conflict) => Self::unique(
                UniqueColumnConstraint::from_ast_conflict(context, table, column, conflict),
            ),
            AstColumnConstraint::Identity(mode) => Self::identity(
                IdentityColumnConstraint::from_ast_mode(context, table, column, mode),
            ),
            AstColumnConstraint::Generated(expression, mode) => {
                Self::generated(GeneratedColumnConstraint::from_ast_expression(
                    context, table, column, expression, mode,
                )?)
            }
            AstColumnConstraint::ForeignKey(reference) => {
                Self::from_ast_foreign_reference(reference, context, table, column)?
            }
            AstColumnConstraint::Exclusion => Self::exclusion(
                ExclusionColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Deferrability(value) => Self::attr_deferrability(
                AttrDeferrabilityColumnConstraint::from_ast_value(context, table, column, *value),
            ),
            AstColumnConstraint::Deferred => Self::attr_deferred(
                AttrDeferredColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Immediate => Self::attr_immediate(
                AttrImmediateColumnConstraint::from_ast_declaration(context, table, column),
            ),
            AstColumnConstraint::Collate(name) => Self::collate(
                CollateColumnConstraint::from_ast_name(context, table, column, name)?,
            ),
        })
    }

    pub(crate) fn from_ast_foreign_reference(
        ast: &ForeignReference,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        let target = TableName::from(FullName::from_ast_name(&ast.table)?);
        #[cfg(feature = "sqlite")]
        Constraint::report_ast_ignored_matches(ast, context, table, column)?;
        let [target_column] = ast.columns.as_slice() else {
            return AnalysisError::raise_unexpected_input("foreign key target column count != 1");
        };
        let target_column = ColumnName::from(target_column.as_str());
        let (binding, target_type) =
            ForeignKeyTarget::resolve_column(context, &target, &target_column)?;
        Ok(Self::foreign_key(ForeignKeyColumnConstraint::new(
            table,
            column,
            binding.relation(),
            &target_column,
            target_type.as_ref(),
            ast.match_type.as_ref(),
            ast.delete_action.clone(),
            ast.update_action.clone(),
            ast.deferrability.clone(),
            ast.validity.clone(),
        )))
    }
}

impl CreateTableAs {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &TableAs,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        Self::from_ast_and_apply_at(ast, context, cache, &source)
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &TableAs,
        mut context: TContext,
        cache: &mut CacheContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut query = SelectStatement::from_ast(&ast.query, &mut context, cache)?;
        let name = FullName::from_ast_name(&ast.into.relation.name)?;
        if ast.into.columns.len() > query.result_columns().len() {
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
        query.rename_columns(&ast.into.columns);
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
            CreateTableMode::from_if_not_exists(ast.if_not_exists),
            ast.into.relation.persistence == "t",
            source,
        );
        let table_as = Self::new(&name, &SemAst::select(&query), ast.materialized)
            .with_if_not_exists(ast.if_not_exists);
        let context = if ast.materialized {
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

impl CreateIndex {
    #[cfg(test)]
    pub(crate) fn elements_from_ast(
        elements: &[IndexExpression],
        context: &mut impl ParseContext,
    ) -> Result<IndexElementCollection, AnalysisError> {
        Self::elements_from_ast_in(elements, context, &ScalarAnalysisContext::default())
    }

    pub(crate) fn elements_from_ast_in(
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
                IndexElement::from(&SemScalarExpr::from_ast_in(expr, context, scope)?)
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

    pub(crate) fn from_ast(
        ast: &Index,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }

    pub(crate) fn from_ast_inner(
        ast: &Index,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let requested_table = TableName::from(FullName::from_ast_name(&ast.relation.name)?);
        let Some(table) = context.resolve_table_name(&requested_table) else {
            context
                .report_analysis_problem(AnalysisProblem::relation_not_found(&requested_table))?;
            return Ok(None);
        };
        let name = FullName::new(
            &table.full_name().catalog().cloned(),
            &table.full_name().schema().cloned(),
            &LocalName::from(ast.name.as_str()),
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
        let keys = Self::elements_from_ast_in(&ast.keys, context, &scope)?;
        let includes = Self::elements_from_ast_in(&ast.includes, context, &scope)?;
        let predicate = SemScalarExpr::from_ast_optional(&ast.predicate, context, &scope)?;
        let mut index = Self::from_resolved(&name, &table, &keys, &includes, ast.unique);
        index = index.with_if_not_exists(ast.if_not_exists);
        index.set_predicate(predicate);
        if !Self::retain_ast_storage(ast, context, &mut index)? {
            return Ok(None);
        }
        Ok(Some(index))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Index,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(index) = Self::from_ast(ast, &mut context)? else {
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

impl CreateSchema {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Schema,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        context.apply_creation_group(|context| Self::from_ast_group(ast, context, cache))
    }

    pub(crate) fn from_ast_group<TContext: ParseContext>(
        ast: &Schema,
        mut context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if context.schema_definition(&ast.name).is_some() {
            let schema = Self::new(&ast.name, &[]).with_if_not_exists(ast.if_not_exists);
            let context = context.apply_create_schema(&schema)?;
            return Ok(AstAndContextPair::new(
                SemAst::create_schema(&schema),
                context,
            ));
        }
        let owner = if let Some(authorization) = &ast.authorization {
            Some(OwnerSpecification::from_ast(authorization, &mut context)?)
        } else {
            None
        };
        let previous_path = context.get_search_path().to_vec();
        let mut schema_path = vec![crate::names::SchemaName::from(ast.name.as_str())];
        for schema in &previous_path {
            if !schema_path.contains(schema) {
                schema_path.push(schema.clone());
            }
        }
        context.set_search_path(&schema_path);
        let mut elements = Vec::new();
        for element in &ast.elements {
            let AstAndContextPair(element, next) = SemAst::apply_ast_node(element, context, cache)?;
            context = next;
            elements.push(element);
        }
        context.set_search_path(&previous_path);
        let mut schema = Self::new(&ast.name, &elements).with_if_not_exists(ast.if_not_exists);
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

impl FunctionParam {
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &FunctionParameter,
        mut context: TContext,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast(ast, &mut context);
        let parameter = context.finish_analysis(result)?;
        Ok((parameter, context))
    }

    pub(crate) fn from_ast(
        ast: &FunctionParameter,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = Some(ast.name.clone());
        let type_name = FullName::from_ast_name(&ast.type_name.name)?;
        let param_type = context.resolve_type_name(&type_name).map(|name| {
            TypeReference::concrete_type_ref(&name, !ast.type_name.array_bounds.is_empty())
        });
        let default = ast
            .default
            .as_ref()
            .map(|expr| {
                SemScalarExpr::from_ast_in(expr, context, &ScalarAnalysisContext::default())
            })
            .transpose()?;
        Ok(Self::from_ast_mode(&ast.mode, &name, &param_type, &default))
    }

    pub(crate) fn from_ast_mode(
        ast: &ParameterMode,
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default: &Option<SemScalarExpr>,
    ) -> Self {
        match ast {
            ParameterMode::In => Self::new_input_param(name, param_type, default),
            ParameterMode::Out => Self::new_output_param(name, param_type),
            ParameterMode::InOut => Self::new_in_out_param(name, param_type, default),
            ParameterMode::Variadic => Self::new_variadig_param(name, param_type),
            ParameterMode::Table => Self::new_table_param(name, param_type),
        }
    }
}

impl CreateFunction {
    pub(crate) fn from_ast(
        ast: &Function,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_function_null_input_semantics {
    use crate::sem::CreateFunction;
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
        let resolved = CreateFunction::from_ast(&function, &mut context).unwrap();
        assert_eq!(
            expected,
            resolved.overloads().get_at(0).unwrap().is_strict()
        );
    }
}

impl CreateFunction {
    pub(crate) fn from_ast_inner(
        ast: &Function,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(&ast.name)?;
        if ast.procedure || ast.result_type.is_none() {
            let mut params = Vec::new();
            let mut outputs = Vec::new();
            for param in &ast.parameters {
                let resolved = FunctionParam::from_ast(param, context)?;
                if param.mode.is_output() {
                    outputs.push(resolved.get_type().cloned());
                }
                params.push(resolved);
            }
            let ret_type = if ast.procedure {
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
            let strict = ast.options.iter().any(FunctionOption::is_strict);
            let overload = OverloadVariant::new(&ret_type, &params, strict, false);
            return Ok(
                Self::new(&name, &FunctionOverloadCollection::new(&[overload]), &None)
                    .with_procedure_kind(ast.procedure),
            );
        }
        let Some(result_type) = &ast.result_type else {
            return AnalysisError::raise_unexpected_none("create_function.return_type");
        };
        let ret_name = FullName::from_ast_name(&result_type.name)?;
        let returning = if result_type.set_of {
            if ret_name.name().as_str() == "record" {
                let mut columns = Vec::new();
                for param in &ast.parameters {
                    if param.mode.is_output() {
                        let ty = TypeReference::from_full_name(FullName::from_ast_name(
                            &param.type_name.name,
                        )?);
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
        for param in &ast.parameters {
            params.push(FunctionParam::from_ast(param, context)?);
        }
        let ret_type = if let Some(ty) = context.get_type(&ret_name) {
            ty.type_reference().cloned()
        } else {
            None
        };
        let strict = ast.options.iter().any(FunctionOption::is_strict);
        let overload = OverloadVariant::new(&ret_type, &params, strict, false);
        Ok(Self::new(
            &name,
            &FunctionOverloadCollection::new(&[overload]),
            &returning,
        ))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Function,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let function = Self::from_ast(ast, &mut context)?.with_replace(ast.replace);
        let preserves_rows = ast.result_type.as_ref().is_none_or(|result| !result.set_of)
            && !ast
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

impl CreateCompositeType {
    pub(crate) fn from_ast(
        ast: &CompositeType,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(&ast.name)?;
        let table = TableName::from(name.clone());
        let mut fields = CompositeFieldCollection::default();
        for field in &ast.fields {
            fields.push(CompositeFieldDefinition::from_ast(field, context, &table)?);
        }
        Ok(Self::new(&name, &fields))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &CompositeType,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let composite = Self::from_ast(ast, &mut context)?;
        let context = context.apply_create_comp_type(&composite)?;
        Ok(AstAndContextPair::new(
            SemAst::create_composite_type(&composite),
            context,
        ))
    }
}

impl CompositeFieldDefinition {
    pub(crate) fn from_ast(
        ast: &CompositeField,
        context: &mut impl ParseContext,
        composite: &TableName,
    ) -> Result<Self, AnalysisError> {
        let requested = TypeReference::from_ast(&ast.type_name)?;
        let type_ref = context.canonicalize_type_ref(&requested);
        if type_ref.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                composite,
                &ColumnName::from(ast.name.as_str()),
                requested
                    .full_name()
                    .expect("SQL type syntax constructs a concrete reference"),
            ))?;
        }
        let mut field = Self::new(&ast.name, &type_ref, &Some(ast.is_not_null));
        field.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
            &ast.type_name,
            type_ref.as_ref(),
        )?);
        let lookup = ast
            .collation
            .as_ref()
            .map(FullName::from_ast_name)
            .transpose()?
            .map(|name| context.lookup_collation(&name));
        field.validate_collation(context, lookup.as_ref())?;
        Ok(field)
    }
}

#[cfg(test)]
mod test_composite_field_resolution {
    use crate::names::{FullName, TableName};
    use crate::sem::CompositeFieldDefinition;

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
        let field = CompositeFieldDefinition::from_ast(
            &composite.fields[0],
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

impl CreateDomain {
    pub(crate) fn from_ast(
        ast: &Domain,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod domain_tests {
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
        let domain = SemAst::from_ast_node(&syntax, &mut context, &mut create_cache_context())?;
        assert!(matches!(domain, SemAst::CreateDomain(_)));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl CreateDomain {
    pub(crate) fn from_ast_inner(
        ast: &Domain,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(&ast.name)?;
        let base_name = FullName::from_ast_name(&ast.base.name)?;
        let base = context.canonicalize_type_ref(&TypeReference::from_ast(&ast.base)?);
        if base.is_none() {
            context.report_analysis_problem(AnalysisProblem::domain_base_type_not_found(
                &name,
                &base_name.to_string(),
                &ast.base_span,
            ))?;
        }
        let mut domain = Self::new(&TypeReference::from_full_name(name), &base);
        let category = if let Some(base) = context.get_type(&base_name) {
            if ast.base.array_bounds.is_empty() {
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
        domain.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
            &ast.base,
            domain.base_type(),
        )?);
        let lookup = FullName::from_optional_ast_name(&ast.collation)?
            .map(|name| context.lookup_collation(&name));
        domain.validate_collation(context, lookup.as_ref())?;
        Ok(domain)
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Domain,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::domain(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let domain = Self::from_ast(ast, &mut context)?;
        let context = context.apply_create_domain(&domain)?;
        Ok(AstAndContextPair::new(
            SemAst::create_domain(&domain),
            context,
        ))
    }
}

impl AlterView {
    pub(crate) fn from_ast_defaults(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        let requested = FullName::from_ast_name(&ast.name)?;
        let Some(name) = expand_relation_search_path(context, &requested, |key| {
            context
                .get_view_impl(key)
                .filter(|view| !view.is_materialized())
                .map(|_| key.clone())
        }) else {
            return Ok(None);
        };
        let mut actions = Vec::new();
        for command in &ast.commands {
            let Some(action) = AlterObjSubCommand::from_ast_view_default_action(command, context)?
            else {
                return Ok(None);
            };
            actions.push(action);
        }
        let change = Self::new(&ViewName::from_full_name(&name), &actions);
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

impl AlterObjSubCommand {
    pub(crate) fn from_ast_view_default_action(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            AlterCommand::Postgres(command) => {
                Self::from_ast_postgres_view_default_action(command, context)
            }
            AlterCommand::Default(name, expression) => {
                Self::from_ast_view_default(context, name, expression).map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn from_ast_postgres_view_default_action(
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast.operation {
            AlterOperation::ColumnDefault => {
                Self::from_ast_postgres_view_column_default(ast, context)
            }
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

    fn from_ast_postgres_view_column_default(
        ast: &PostgresAlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        let value = SemScalarExpr::from_ast_view_default_definition(&ast.definition, context)?;
        let column = ColumnName::from(ast.name.as_str());
        let action = match value {
            Some(value) => Self::column_default(&column, &value),
            None => Self::drop_column_default(&column),
        };
        Ok(Some(action))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_view_default_definition(
        ast: &AlterDefinition,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            AlterDefinition::None => Ok(None),
            AlterDefinition::Expression(value) => {
                Self::from_ast_view_default_expression(value, context)
            }
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
        let value = SemScalarExpr::from_ast(
            expression,
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )?;
        Ok(Self::column_default(&ColumnName::from(name), &value))
    }
}

impl SemScalarExpr {
    fn from_ast_view_default_expression(
        ast: &Expr,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, ResolutionFailure> {
        Self::from_ast(
            ast,
            context,
            &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
        )
        .map(Some)
        .map_err(Into::into)
    }
}

impl EngineFeatureValidation {
    pub(crate) fn alter_command(
        ast: &AlterCommand,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match ast {
            AlterCommand::Postgres(command) => Self::alter_definition(&command.definition, context),
            AlterCommand::Constraint(constraint) => {
                Self::table_constraint(constraint, context, false)
            }
            _ => Ok(true),
        }
    }

    pub(crate) fn alter_definition(
        ast: &AlterDefinition,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match ast {
            AlterDefinition::Constraint(constraint) => {
                Self::table_constraint(constraint, context, false)
            }
            AlterDefinition::Column(column) => Self::column(column, context),
            AlterDefinition::ConstraintFlags(flags) => Self::alter_constraint_flags(flags, context),
            _ => Ok(true),
        }
    }

    pub(crate) fn alter_constraint_flags(
        ast: &AlterConstraintFlags,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        use crate::engine_specific::{Postgres18Feature, Postgres18Syntax};
        let mut valid = true;
        if ast.enforced.is_some() {
            valid &= context.postgres18_syntax_used(Postgres18Syntax::ConstraintEnforcement)?;
        }
        if ast.no_inherit.is_some() {
            valid &= context.postgres18_syntax_used(Postgres18Syntax::ConstraintInheritance)?;
        }
        if valid && (ast.enforced.is_some() || ast.no_inherit.is_some()) {
            todo!("ALTER CONSTRAINT enforcement/inheritance (#887)");
        }
        Ok(valid)
    }

    pub(crate) fn domain(
        ast: &Domain,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for constraint in &ast.constraints {
            valid &= Self::table_constraint(constraint, context, true)?;
        }
        if valid
            && ast.constraints.iter().any(|constraint| {
                matches!(constraint.constraint, AstColumnConstraint::Enforcement(_))
            })
        {
            todo!("domain constraint analysis (#548)");
        }
        Ok(valid)
    }
}
