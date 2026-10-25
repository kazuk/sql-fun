//! Owned DDL syntax. Catalog bindings are obtained only during resolution.

mod alter_index;
mod column_attributes;

#[path = "deparse/ddl.rs"]
mod sql_emission;

use crate::{
    AstAndContextPair, StringSpan,
    context::ParseContext,
    sem::{
        AliasSpec, AlterIndex, AlterObjSubCommand, AlterTable, AlterView, AnalysisError,
        AnalysisProblem, AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint, CheckColumnConstraint, CollateColumnConstraint,
        ColumnCollection, ColumnConstraint, ColumnDefinition, ColumnName,
        ColumnNullabilityConstraint, CompositeFieldCollection, CompositeFieldDefinition,
        Constraint, CreateCompositeType, CreateDomain, CreateFunction, CreateIndex, CreateSchema,
        CreateTable, CreateTableAs, CreateTableMode, DataSource, DefaultColumnConstraint,
        ExclusionColumnConstraint, ForeignKeyColumnConstraint, ForeignKeyConstraint, FromClause,
        FullName, FunctionOverloadCollection, FunctionParam, GeneratedColumnConstraint,
        IdentityColumnConstraint, IndexElement, IndexElementCollection, IndexElementWithOrder,
        Kind, LocalName, OverloadVariant, PrimaryKeyColumnConstraint, PrimaryKeyConstraint,
        ResolutionFailure, ScalarAnalysisContext, SemAst, SemScalarExpr, SortOrder,
        SourcePathWithSpan, TableName, TypeCategory, TypeReference, UniqueColumnConstraint,
        UniqueConstraint, ViewName, WithClause,
        problem::{CreationProblem, ViewProblem},
        search_path::expand_relation_search_path,
    },
};

use super::{Dialect, Expr, Name, StatementNode, SyntaxOption, TypeName};

impl From<&FullName> for Name {
    fn from(value: &FullName) -> Self {
        let mut parts = Vec::new();
        if let Some(catalog) = value.catalog() {
            parts.push(catalog.to_string());
        }
        if let Some(schema) = value.schema() {
            parts.push(schema.to_string());
        }
        parts.push(value.name().to_string());
        Self::from_parts(&parts)
    }
}

impl super::AlterRelation {
    pub(crate) const fn with_view_kind(mut self, materialized: bool) -> Self {
        self.kind = super::AlterRelationKind::view(materialized);
        self
    }
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

    fn resolve_and_apply_with_source<TContext: ParseContext>(
        &self,
        mut context: TContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
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

    fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        if self.kind.supports_table_column_attributes() && self.has_column_attributes() {
            return self.resolve_column_attributes(context, source);
        }
        self.kind.resolve(
            context,
            &self.name.full_name()?,
            &self.commands,
            source,
            self.if_exists,
        )
    }
}

impl super::AlterRelationKind {
    pub(super) const fn table() -> Self {
        Self::Table
    }

    const fn view(materialized: bool) -> Self {
        if materialized {
            Self::MaterializedView
        } else {
            Self::View
        }
    }

    pub(super) const fn supports_only(self) -> bool {
        matches!(self, Self::Table | Self::ForeignTable)
    }

    fn resolve(
        self,
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[super::AlterCommand],
        source: Option<&SourcePathWithSpan>,
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        match self {
            Self::Table => Self::resolve_table(context, name, syntax_commands, source),
            Self::View => Self::resolve_view(context, name, syntax_commands, false),
            Self::MaterializedView => Self::resolve_view(context, name, syntax_commands, true),
            Self::Sequence => Self::resolve_sequence(context, name, syntax_commands),
            Self::Index => Self::resolve_index(context, name, syntax_commands, if_exists),
            Self::ForeignTable | Self::CompositeType => todo!(),
        }
    }

    fn resolve_index(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[super::AlterCommand],
        if_exists: bool,
    ) -> Result<SemAst, AnalysisError> {
        let mut operations = Vec::new();
        for command in syntax_commands {
            operations.push(command.resolve_index(context)?);
        }
        let statement = AlterIndex::resolve(context, name, if_exists, &operations)?;
        Ok(SemAst::alter_index(&statement))
    }

    fn resolve_table(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[super::AlterCommand],
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
            commands.push(command.resolve(context, &table)?);
        }
        Ok(SemAst::alter_table(&AlterTable::new(
            &table, &commands, source,
        )))
    }

    fn resolve_view(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[super::AlterCommand],
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
            .map(|command| command.resolve_owner(context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_view(&AlterView::new(&view, &commands)))
    }

    fn resolve_sequence(
        context: &mut impl ParseContext,
        name: &FullName,
        syntax_commands: &[super::AlterCommand],
    ) -> Result<SemAst, AnalysisError> {
        if context.get_sequence(name).is_none() {
            context.report_analysis_problem(AnalysisProblem::sequence_not_found(name))?;
        }
        let commands = syntax_commands
            .iter()
            .map(|command| command.resolve_owner(context))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SemAst::alter_sequence(&crate::sem::AlterSequence::new(
            name, &commands,
        )))
    }
}

impl super::AlterCommand {
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
    fn resolve_owner(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        match self {
            Self::Postgres(command) => command.resolve_owner(context),
            Self::Owner(owner) => Ok(AlterObjSubCommand::resolved_owner(
                &owner.resolve_owner(context)?,
            )),
            _ => todo!(),
        }
    }

    fn missing_column(
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

    fn existing_column(
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

    fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        match self {
            Self::Postgres(command) => command.resolve(context, table),
            Self::Owner(_) => self.resolve_owner(context),
            Self::Default(name, expr) => Self::resolve_default(context, table, name, expr),
            Self::Constraint(constraint) => Self::resolve_constraint(context, table, constraint),
            Self::Cluster(name) => Self::resolve_cluster(context, table, name),
            Self::Rename(name) => Self::resolve_rename(context, name),
            Self::AddColumn(column) if context.dialect() == Dialect::sqlite() => {
                Self::resolve_add_column(context, table, column)
            }
            Self::AlterColumn(name, column) if context.dialect() == Dialect::sqlite() => {
                Self::resolve_alter_column(context, table, name, column)
            }
            Self::RenameColumn(from, to) => Self::resolve_rename_column(context, table, from, to),
            Self::DropColumn(name) if context.dialect() == Dialect::sqlite() => {
                Self::resolve_drop_column(context, table, name)
            }
            _ => todo!(),
        }
    }

    fn resolve_default(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        expr: &Expr,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        let expr = expr.resolve_in(context, &ScalarAnalysisContext::default())?;
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::column_default(&name, &expr))
    }

    fn resolve_constraint(
        context: &mut impl ParseContext,
        table: &TableName,
        constraint: &super::TableConstraint,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        // Constraint-created indexes need the matched key even when the stored
        // table declaration remains unqualified.
        let resolved_table = context.resolve_table_name(table);
        let table = resolved_table.as_ref().unwrap_or(table);
        Ok(AlterObjSubCommand::add_constraint(
            &constraint.resolve(context, table)?,
        ))
    }

    fn resolve_cluster(
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

    fn resolve_rename(
        context: &mut impl ParseContext,
        name: &Name,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = TableName::from(name.full_name()?);
        if context.get_table(&name).is_some() {
            context.report_analysis_problem(AnalysisProblem::relation_already_exists(&name))?;
        }
        Ok(AlterObjSubCommand::rename_to(&name))
    }

    fn resolve_add_column(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &super::Column,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let column = column.resolve(context, table)?;
        if let Some(name) = column.name() {
            Self::existing_column(context, table, name)?;
        }
        Ok(AlterObjSubCommand::add_column(&column))
    }

    fn resolve_alter_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        column: &super::Column,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::alter_column(
            &name,
            &column.resolve(context, table)?,
        ))
    }

    fn resolve_rename_column(
        context: &mut impl ParseContext,
        table: &TableName,
        from: &str,
        to: &str,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let from = ColumnName::from(from);
        let to = ColumnName::from(to);
        Self::missing_column(context, table, &from)?;
        if from != to {
            Self::existing_column(context, table, &to)?;
        }
        Ok(AlterObjSubCommand::rename_column(&from, &to))
    }

    fn resolve_drop_column(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let name = ColumnName::from(name);
        Self::missing_column(context, table, &name)?;
        Ok(AlterObjSubCommand::drop_column(&name))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PostgresAlterCommand {
    operation: AlterOperation,
    name: String,
    number: i32,
    definition: AlterDefinition,
    missing_ok: bool,
    cascade: bool,
    legacy: Option<Box<super::AlterCommand>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AlterOperation {
    AddColumn,
    ColumnDefault,
    DropNotNull,
    SetNotNull,
    SetExpression,
    DropExpression,
    SetStatistics,
    SetOptions,
    ResetOptions,
    SetStorage,
    SetCompression,
    DropColumn,
    AddConstraint,
    AlterConstraint,
    ValidateConstraint,
    DropConstraint,
    AlterColumnType,
    AlterColumnGenericOptions,
    Owner,
    Cluster,
    DropCluster,
    SetLogged,
    SetUnlogged,
    DropOids,
    SetAccessMethod,
    SetTablespace,
    SetRelOptions,
    ResetRelOptions,
    EnableTrigger,
    EnableAlwaysTrigger,
    EnableReplicaTrigger,
    DisableTrigger,
    EnableTriggerAll,
    DisableTriggerAll,
    EnableTriggerUser,
    DisableTriggerUser,
    EnableRule,
    EnableAlwaysRule,
    EnableReplicaRule,
    DisableRule,
    AddInherit,
    DropInherit,
    AddOf,
    DropOf,
    ReplicaIdentity,
    EnableRowSecurity,
    DisableRowSecurity,
    ForceRowSecurity,
    NoForceRowSecurity,
    GenericOptions,
    AttachPartition,
    DetachPartition,
    DetachPartitionFinalize,
    AddIdentity,
    SetIdentity,
    DropIdentity,
}

#[derive(Clone, Debug)]
pub(crate) enum AlterDefinition {
    None,
    Expression(Expr),
    Text(String),
    Options(Vec<SyntaxOption>),
    GenericOptions(Vec<GenericOption>),
    Column(super::Column),
    ColumnType(AlterColumnType),
    Constraint(super::TableConstraint),
    ConstraintFlags(AlterConstraintFlags),
    Role(super::Role),
    Relation(Name),
    Type(TypeName),
    Replica(ReplicaIdentity),
    Partition(AlterPartition),
    Identity(AlterIdentity),
}

impl AlterDefinition {
    pub(super) const fn none() -> Self {
        Self::None
    }

    pub(super) const fn expression(value: Expr) -> Self {
        Self::Expression(value)
    }

    pub(super) fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    pub(super) const fn options(values: Vec<SyntaxOption>) -> Self {
        Self::Options(values)
    }

    pub(super) const fn generic_options(values: Vec<GenericOption>) -> Self {
        Self::GenericOptions(values)
    }

    pub(super) const fn column(value: super::Column) -> Self {
        Self::Column(value)
    }

    pub(super) const fn column_type(value: AlterColumnType) -> Self {
        Self::ColumnType(value)
    }

    pub(super) const fn constraint(value: super::TableConstraint) -> Self {
        Self::Constraint(value)
    }

    pub(super) const fn constraint_flags(value: AlterConstraintFlags) -> Self {
        Self::ConstraintFlags(value)
    }

    pub(super) const fn role(value: super::Role) -> Self {
        Self::Role(value)
    }

    pub(super) const fn relation(value: Name) -> Self {
        Self::Relation(value)
    }

    pub(super) const fn type_name(value: TypeName) -> Self {
        Self::Type(value)
    }

    pub(super) const fn replica(value: ReplicaIdentity) -> Self {
        Self::Replica(value)
    }

    pub(super) const fn partition(value: AlterPartition) -> Self {
        Self::Partition(value)
    }

    pub(super) const fn identity(value: AlterIdentity) -> Self {
        Self::Identity(value)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterColumnType {
    type_name: TypeName,
    collation: Option<Name>,
    using: Option<Expr>,
}
#[derive(Clone, Debug)]
pub(crate) struct AlterConstraintFlags {
    name: String,
    deferrable: bool,
    initially_deferred: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct GenericOption {
    action: GenericAction,
    name: String,
    value: Option<String>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum GenericAction {
    Unspecified,
    Add,
    Set,
    Drop,
}
#[derive(Clone, Debug)]
pub(crate) struct ReplicaIdentity {
    kind: String,
    index: String,
}
#[derive(Clone, Debug)]
pub(crate) struct AlterPartition {
    relation: Name,
    bound: Option<super::PartitionBound>,
    concurrent: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct AlterIdentity {
    mode: String,
    options: Vec<SyntaxOption>,
}

impl PostgresAlterCommand {
    pub(super) fn new(
        operation: AlterOperation,
        name: &str,
        number: i32,
        definition: &AlterDefinition,
        missing_ok: bool,
        cascade: bool,
        legacy: Option<&super::AlterCommand>,
    ) -> Self {
        Self {
            operation,
            name: name.to_owned(),
            number,
            definition: definition.clone(),
            missing_ok,
            cascade,
            legacy: legacy.cloned().map(Box::new),
        }
    }
    fn resolve_owner(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        if let Some(legacy) = &self.legacy {
            legacy.resolve_owner(context)
        } else {
            todo!()
        }
    }
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        if self.operation.column_attribute_phase().is_some() {
            return self.resolve_single_column_attribute(context, table);
        }
        if let Some(legacy) = &self.legacy {
            legacy.resolve(context, table)
        } else {
            todo!()
        }
    }
}

impl AlterColumnType {
    pub(super) fn new(
        type_name: &TypeName,
        collation: Option<&Name>,
        using: Option<&Expr>,
    ) -> Self {
        Self {
            type_name: type_name.clone(),
            collation: collation.cloned(),
            using: using.cloned(),
        }
    }
}
impl AlterConstraintFlags {
    pub(super) fn new(name: &str, deferrable: bool, initially_deferred: bool) -> Self {
        Self {
            name: name.to_owned(),
            deferrable,
            initially_deferred,
        }
    }
}
impl GenericOption {
    pub(super) fn new(action: GenericAction, name: &str, value: Option<&str>) -> Self {
        Self {
            action,
            name: name.to_owned(),
            value: value.map(str::to_owned),
        }
    }
}
impl ReplicaIdentity {
    pub(super) fn new(kind: &str, index: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            index: index.to_owned(),
        }
    }
}
impl AlterPartition {
    pub(super) fn new(
        relation: &Name,
        bound: Option<&super::PartitionBound>,
        concurrent: bool,
    ) -> Self {
        Self {
            relation: relation.clone(),
            bound: bound.cloned(),
            concurrent,
        }
    }
}
impl AlterIdentity {
    pub(super) fn new(mode: &str, options: &[SyntaxOption]) -> Self {
        Self {
            mode: mode.to_owned(),
            options: options.to_vec(),
        }
    }
}

impl super::AlterCommand {
    pub(super) fn postgres(command: PostgresAlterCommand) -> Self {
        Self::Postgres(Box::new(command))
    }
}

impl super::TableConstraint {
    fn resolve_declared_key(
        &self,
        context: &mut impl ParseContext,
        table: &CreateTable,
    ) -> Result<Option<Constraint>, AnalysisError> {
        if !self.constraint.is_unique_key() {
            return Ok(None);
        }
        if self.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let keys: Vec<_> = self
            .keys
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect();
        for key in &keys {
            if !table.has_column(key) {
                context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                    table.name(),
                    key,
                ))?;
            }
        }
        self.constraint
            .resolve_table(
                context,
                table.name(),
                self.name.as_deref().unwrap_or_default(),
                &keys,
            )
            .map(Some)
    }
}

impl super::TableConstraint {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Constraint, AnalysisError> {
        let keys = self
            .keys
            .iter()
            .map(|key| ColumnName::from(key.as_str()))
            .collect::<Vec<_>>();
        if context.get_table(table).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(table))?;
        }
        for key in &keys {
            super::AlterCommand::missing_column(context, table, key)?;
        }
        if self.constraint.is_unique_key() && self.extras.deferrable {
            todo!("deferrable unique-key catalog representation");
        }
        let name = self.name.as_deref().unwrap_or_default();
        self.constraint.resolve_table(context, table, name, &keys)
    }
}
impl super::ColumnConstraint {
    fn has_catalog_name(&self, context: &impl ParseContext) -> bool {
        match self {
            Self::PrimaryKey(..)
            | Self::Unique(_)
            | Self::Check(_)
            | Self::ForeignKey(_)
            | Self::Exclusion => true,
            Self::Nullability(true, ..) => {
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
            _ => false,
        }
    }
    const fn is_unique_key(&self) -> bool {
        matches!(self, Self::PrimaryKey(_, _) | Self::Unique(_))
    }
}

impl super::ColumnConstraint {
    fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Constraint, AnalysisError> {
        match self {
            Self::PrimaryKey(_, _) => Ok(Self::resolve_table_primary_key(table, name, keys)),
            Self::Unique(_) => Ok(Self::resolve_table_unique(table, name, keys)),
            Self::ForeignKey(reference) => reference.resolve_table(context, table, name, keys),
            _ => todo!(),
        }
    }

    fn resolve_table_primary_key(table: &TableName, name: &str, keys: &[ColumnName]) -> Constraint {
        Constraint::primary_key(&PrimaryKeyConstraint::new(name, table, keys))
    }

    fn resolve_table_unique(table: &TableName, name: &str, keys: &[ColumnName]) -> Constraint {
        Constraint::unique(&UniqueConstraint::new(name, table, keys))
    }
}

impl super::ForeignReference {
    fn resolve_table(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        keys: &[ColumnName],
    ) -> Result<Constraint, AnalysisError> {
        let target = TableName::from(self.table.full_name()?);
        if context.get_table(&target).is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&target))?;
        }
        let target_keys = if self.columns.is_empty() {
            context
                .get_table(&target)
                .map(|table| table.primary_key_columns().into_iter().cloned().collect())
        } else {
            Some(
                self.columns
                    .iter()
                    .map(|key| ColumnName::from(key.as_str()))
                    .collect::<Vec<_>>(),
            )
        };
        if let Some(keys) = &target_keys {
            for key in keys {
                super::AlterCommand::missing_column(context, &target, key)?;
            }
        }
        Ok(Constraint::foreign(&ForeignKeyConstraint::new(
            name,
            table,
            &target,
            keys,
            &target_keys,
        )))
    }
}

impl super::Table {
    pub(crate) fn named(mut self, name: &FullName, mode: CreateTableMode, temporary: bool) -> Self {
        self.name = Name::from(name);
        self.mode = mode;
        self.temporary = temporary;
        self
    }
    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        self.resolve_and_apply_at(context, &source)
    }

    fn duplicate_constraint_name(&self, context: &impl ParseContext) -> Option<&str> {
        let mut names = Vec::new();
        for definition in &self.constraints {
            if let Some(name) = definition.name.as_deref() {
                if names.contains(&name) {
                    return Some(name);
                }
                names.push(name);
            }
        }
        for column in &self.columns {
            for definition in &column.constraints {
                if definition.constraint.has_catalog_name(context)
                    && let Some(name) = definition.name.as_deref()
                {
                    if names.contains(&name) {
                        return Some(name);
                    }
                    names.push(name);
                }
            }
        }
        None
    }
    pub(crate) fn resolve_and_apply_at<TContext: ParseContext>(
        &self,
        mut context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let name = context.creation_name(&self.name.full_name()?, self.temporary)?;
        if !context.permits_relation_creation(
            &name,
            Kind::table(),
            self.mode.if_not_exists(),
            false,
        )? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        if context.dialect() == Dialect::postgres()
            && let Some(constraint) = self.duplicate_constraint_name(&context)
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
        let table = self.resolve(&mut context, source)?;
        let context = context.apply_create_table(&table)?;
        Ok(AstAndContextPair::new(
            SemAst::create_table(&table),
            context,
        ))
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<CreateTable, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, source);
        context.finish_analysis(result)
    }

    fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<CreateTable, AnalysisError> {
        if self.query.is_some() {
            todo!()
        }
        let name = TableName::from(self.name.full_name()?);
        let mut columns = Vec::new();
        for column in &self.columns {
            columns.push(column.resolve(context, &name)?);
        }
        let mut table = CreateTable::new(&name, columns, self.mode, self.temporary, source);
        for definition in &self.constraints {
            if let Some(constraint) = definition.resolve_declared_key(context, &table)? {
                table.record_declared_key(&constraint)?;
            }
        }
        for column in &self.columns {
            for definition in &column.constraints {
                if definition.name.is_some() && definition.constraint.is_unique_key() {
                    let keys = [ColumnName::from(column.name.as_str())];
                    let constraint = definition.constraint.resolve_table(
                        context,
                        &name,
                        definition.name.as_deref().expect("named inline key"),
                        &keys,
                    )?;
                    table.record_declared_key(&constraint)?;
                }
            }
        }
        if context.dialect() == crate::ast::Dialect::postgres() {
            table.apply_primary_key_nullability();
        }
        Ok(table)
    }
}

impl super::Column {
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(ColumnDefinition, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(&mut context, table);
        let column = context.finish_analysis(result)?;
        Ok((column, context))
    }
    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<ColumnDefinition, AnalysisError> {
        let name = ColumnName::from(self.name.as_str());
        let type_ref = if let Some(ty) = &self.type_name {
            let type_name = ty.name.full_name()?;
            if let Some(ty) = context.get_type(&type_name) {
                if context.dialect() == Dialect::sqlite() {
                    Some(TypeReference::from_full_name(type_name))
                } else if let Some(type_ref) = ty.type_reference() {
                    let array = self
                        .type_name
                        .as_ref()
                        .is_some_and(|syntax| !syntax.array_bounds.is_empty());
                    Some(TypeReference::concrete_type_ref(
                        type_ref.full_name(),
                        array,
                    ))
                } else {
                    let problem = AnalysisProblem::unexpected_dynamic_type(ty);
                    context.report_analysis_problem(problem)?;
                    None
                }
            } else {
                context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                    table, &name, &type_name,
                ))?;
                None
            }
        } else {
            None
        };
        let mut column =
            ColumnDefinition::new(&Some(name.clone()), type_ref.as_ref(), self.not_null, &[]);
        for constraint in &self.constraints {
            if constraint.constraint.is_unique_key() && constraint.extras.deferrable {
                todo!("deferrable unique-key catalog representation");
            }
            let constraint = constraint.constraint.resolve(context, table, &name)?;
            if let Some(nullability) = constraint.as_nullability() {
                column.set_not_null(Some(nullability.is_not_null()));
            }
            if context.dialect() == Dialect::sqlite() {
                if let Some(default) = constraint.as_default() {
                    column.set_default(default.expr());
                }
            } else if let Some(fk) = constraint.as_foreign_key()
                && let Some(col_type) = &type_ref
                && let Some(pk_type) = fk.primary_key_column_type()
                && col_type != pk_type
            {
                context.report_analysis_problem(
                    AnalysisProblem::foreign_key_column_type_mismatch(
                        table,
                        &name,
                        col_type,
                        fk.primary_key_table(),
                        fk.primary_key_column(),
                        pk_type,
                    ),
                )?;
            }
            column.push_constraint(constraint);
        }
        Ok(column)
    }
}

impl super::ColumnConstraint {
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
    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnConstraint, AnalysisError> {
        let scope = ScalarAnalysisContext::default();
        Ok(match self {
            Self::Nullability(not_null, conflict, validity) => {
                ColumnConstraint::nullability(ColumnNullabilityConstraint::new(
                    table,
                    column,
                    *not_null,
                    conflict.clone(),
                    validity.clone(),
                ))
            }
            Self::Default(expr) => ColumnConstraint::default(DefaultColumnConstraint::new(
                table,
                column,
                expr.resolve_in(context, &scope)?,
            )),
            Self::Check(expr) => ColumnConstraint::check(CheckColumnConstraint::new(
                table,
                column,
                expr.resolve_in(context, &scope)?,
            )),
            Self::PrimaryKey(conflict, deferrable) => {
                ColumnConstraint::primary_key(PrimaryKeyColumnConstraint::new(
                    table,
                    column,
                    conflict.clone(),
                    deferrable.clone(),
                ))
            }
            Self::Unique(conflict) => ColumnConstraint::unique(UniqueColumnConstraint::new(
                table,
                column,
                conflict.clone(),
            )),
            Self::Identity(mode) => ColumnConstraint::identity(IdentityColumnConstraint::new(
                table,
                column,
                mode.clone(),
            )),
            Self::Generated(expr, mode) => {
                ColumnConstraint::generated(GeneratedColumnConstraint::new(
                    table,
                    column,
                    expr.resolve_in(context, &ScalarAnalysisContext::default())?,
                    mode.clone(),
                ))
            }
            Self::ForeignKey(reference) => reference.resolve(context, table, column)?,
            Self::Exclusion => {
                ColumnConstraint::exclusion(ExclusionColumnConstraint::new(table, column))
            }
            Self::Deferrability(value) => ColumnConstraint::attr_deferrability(
                AttrDeferrabilityColumnConstraint::new(table, column, *value),
            ),
            Self::Deferred => {
                ColumnConstraint::attr_deferred(AttrDeferredColumnConstraint::new(table, column))
            }
            Self::Immediate => {
                ColumnConstraint::attr_immediate(AttrImmediateColumnConstraint::new(table, column))
            }
            Self::Collate(name) => {
                if !CollateColumnConstraint::is_known_name(name) {
                    context
                        .report_analysis_problem(AnalysisProblem::collation_name_not_found(name))?;
                }
                ColumnConstraint::collate(CollateColumnConstraint::new(table, column, name))
            }
        })
    }
}

impl super::ForeignReference {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnConstraint, AnalysisError> {
        let target = TableName::from(self.table.full_name()?);
        #[cfg(feature = "sqlite")]
        for name in &self.ignored_matches {
            context.report_analysis_problem(
                AnalysisProblem::sqlite_ignores_match_option_in_fk_constraint(table, column, name),
            )?;
        }
        let [target_column] = self.columns.as_slice() else {
            return AnalysisError::raise_unexpected_input("foreign key target column count != 1");
        };
        let target_column = ColumnName::from(target_column.as_str());
        let target_type = if let Some(target_table) = context.get_table(&target) {
            if let Some(column) = target_table.get_column_def(&target_column) {
                column.get_type_cloned()
            } else {
                context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                    &target,
                    &target_column,
                ))?;
                None
            }
        } else {
            context.report_analysis_problem(AnalysisProblem::table_not_found(&target))?;
            None
        };
        Ok(ColumnConstraint::foreign_key(
            ForeignKeyColumnConstraint::new(
                table,
                column,
                &target,
                &target_column,
                target_type.as_ref(),
                self.match_type.as_ref(),
                self.delete_action.clone(),
                self.update_action.clone(),
                self.deferrability.clone(),
                self.validity.clone(),
            ),
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TableAs {
    into: super::Into,
    query: super::Query,
    materialized: bool,
    if_not_exists: bool,
    skip_data: bool,
}

impl TableAs {
    pub(super) const fn new(
        into: super::Into,
        query: super::Query,
        materialized: bool,
        if_not_exists: bool,
    ) -> Self {
        Self {
            into,
            query,
            materialized,
            if_not_exists,
            skip_data: false,
        }
    }

    pub(super) const fn with_skip_data(mut self, skip_data: bool) -> Self {
        self.skip_data = skip_data;
        self
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        self.resolve_and_apply_at(context, &source)
    }

    pub(crate) fn resolve_and_apply_at<TContext: ParseContext>(
        &self,
        mut context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut query = self.query.resolve(&mut context)?;
        let name = self.into.relation.name.full_name()?;
        if self.into.columns.len() > query.result_columns().len() {
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
        query.rename_columns(&self.into.columns);
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
            CreateTableMode::from_if_not_exists(self.if_not_exists),
            self.into.relation.persistence == "t",
            source,
        );
        let table_as = CreateTableAs::new(&name, &SemAst::select(&query), self.materialized)
            .with_if_not_exists(self.if_not_exists);
        let context = if self.materialized {
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

#[derive(Clone, Debug)]
pub(crate) struct Index {
    name: String,
    relation: super::Relation,
    keys: Vec<super::IndexExpression>,
    includes: Vec<super::IndexExpression>,
    unique: bool,
    predicate: Option<Expr>,
    options: Vec<SyntaxOption>,
    access_method: String,
    concurrent: bool,
    if_not_exists: bool,
    nulls_not_distinct: bool,
    tablespace: String,
}

impl Index {
    #[expect(clippy::too_many_arguments)]
    pub(super) fn new(
        name: &str,
        relation: super::Relation,
        keys: Vec<super::IndexExpression>,
        includes: Vec<super::IndexExpression>,
        unique: bool,
        predicate: Option<Expr>,
        options: Vec<SyntaxOption>,
        access_method: &str,
    ) -> Self {
        Self {
            name: name.to_owned(),
            relation,
            keys,
            includes,
            unique,
            predicate,
            options,
            access_method: access_method.to_owned(),
            concurrent: false,
            if_not_exists: false,
            nulls_not_distinct: false,
            tablespace: String::new(),
        }
    }

    pub(super) fn with_syntax_options(
        mut self,
        concurrent: bool,
        if_not_exists: bool,
        nulls_not_distinct: bool,
        tablespace: &str,
    ) -> Self {
        self.concurrent = concurrent;
        self.if_not_exists = if_not_exists;
        self.nulls_not_distinct = nulls_not_distinct;
        self.tablespace = tablespace.to_owned();
        self
    }

    #[cfg(test)]
    pub(crate) fn resolve_elements(
        elements: &[super::IndexExpression],
        context: &mut impl ParseContext,
    ) -> Result<IndexElementCollection, AnalysisError> {
        Self::resolve_elements_in(elements, context, &ScalarAnalysisContext::default())
    }

    fn resolve_elements_in(
        elements: &[super::IndexExpression],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<IndexElementCollection, AnalysisError> {
        let mut collection = IndexElementCollection::default();
        for element in elements {
            if element.using_order {
                return AnalysisError::raise_unexpected_input("SortbyUsing in IndexElem");
            }
            let value = if let Some(expr) = &element.expression {
                IndexElement::from(&expr.resolve_in(context, scope)?)
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

    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<CreateIndex>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }

    fn resolve_in(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<CreateIndex>, AnalysisError> {
        let requested_table = TableName::from(self.relation.name.full_name()?);
        let Some(table) = context.resolve_table_name(&requested_table) else {
            context
                .report_analysis_problem(AnalysisProblem::relation_not_found(&requested_table))?;
            return Ok(None);
        };
        let name = FullName::new(
            &table.full_name().catalog().cloned(),
            &table.full_name().schema().cloned(),
            &LocalName::from(self.name.as_str()),
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
        let keys = Self::resolve_elements_in(&self.keys, context, &scope)?;
        let includes = Self::resolve_elements_in(&self.includes, context, &scope)?;
        let predicate = Expr::resolve_optional(&self.predicate, context, &scope)?;
        let mut index = CreateIndex::from_resolved(&name, &table, &keys, &includes, self.unique);
        index = index.with_if_not_exists(self.if_not_exists);
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

#[derive(Clone, Debug)]
pub(crate) struct Schema {
    name: String,
    elements: Vec<StatementNode>,
    if_not_exists: bool,
    authorization: Option<super::Role>,
}

impl Schema {
    pub(super) fn new(name: &str, elements: Vec<StatementNode>, if_not_exists: bool) -> Self {
        Self {
            name: name.to_owned(),
            elements,
            if_not_exists,
            authorization: None,
        }
    }

    pub(super) fn with_authorization(mut self, authorization: Option<super::Role>) -> Self {
        self.authorization = authorization;
        self
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        context.apply_creation_group(|context| self.resolve_group(context))
    }
    fn resolve_group<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if context.schema_definition(&self.name).is_some() {
            let schema = CreateSchema::new(&self.name, &[]).with_if_not_exists(self.if_not_exists);
            let context = context.apply_create_schema(&schema)?;
            return Ok(AstAndContextPair::new(
                SemAst::create_schema(&schema),
                context,
            ));
        }
        let owner = if let Some(authorization) = &self.authorization {
            Some(authorization.resolve_owner(&mut context)?)
        } else {
            None
        };
        let previous_path = context.get_search_path().to_vec();
        let mut schema_path = vec![crate::sem::SchemaName::from(self.name.as_str())];
        for schema in &previous_path {
            if !schema_path.contains(schema) {
                schema_path.push(schema.clone());
            }
        }
        context.set_search_path(&schema_path);
        let mut elements = Vec::new();
        for element in &self.elements {
            let AstAndContextPair(element, next) = element.resolve_and_apply(context)?;
            context = next;
            elements.push(element);
        }
        context.set_search_path(&previous_path);
        let mut schema =
            CreateSchema::new(&self.name, &elements).with_if_not_exists(self.if_not_exists);
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

#[derive(Clone, Debug)]
pub(crate) struct Function {
    name: Name,
    parameters: Vec<FunctionParameter>,
    result_type: Option<TypeName>,
    options: Vec<FunctionOption>,
    replace: bool,
    procedure: bool,
    body: Option<FunctionBody>,
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionOption {
    Body(Vec<String>),
    Language(String),
    Transform(Vec<TypeName>),
    Window,
    Volatility(String),
    Leakproof(bool),
    Strict(bool),
    SecurityDefiner(bool),
    Parallel(String),
    Cost(Expr),
    Rows(Expr),
    Support(Name),
    Setting(super::VariableSet),
}

impl FunctionOption {
    const fn is_strict(&self) -> bool {
        matches!(self, Self::Strict(true))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionBody {
    Return(Expr),
    Atomic(Vec<FunctionBodyStatement>),
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionBodyStatement {
    Return(Expr),
    Statement(Box<StatementNode>),
}

impl FunctionBodyStatement {
    pub(super) const fn return_expression(expression: Expr) -> Self {
        Self::Return(expression)
    }

    pub(super) fn statement(statement: StatementNode) -> Self {
        Self::Statement(Box::new(statement))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionParameter {
    name: String,
    type_name: TypeName,
    mode: ParameterMode,
    default: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ParameterMode {
    In,
    Out,
    InOut,
    Variadic,
    Table,
}

impl FunctionParameter {
    pub(crate) fn resolve_and_report<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<(FunctionParam, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve(&mut context);
        let parameter = context.finish_analysis(result)?;
        Ok((parameter, context))
    }
    pub(super) fn new(
        name: &str,
        type_name: TypeName,
        mode: ParameterMode,
        default: Option<Expr>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            type_name,
            mode,
            default,
        }
    }

    fn resolve(&self, context: &mut impl ParseContext) -> Result<FunctionParam, AnalysisError> {
        let name = Some(self.name.clone());
        let type_name = self.type_name.name.full_name()?;
        let param_type = context.resolve_type_name(&type_name).map(|name| {
            TypeReference::concrete_type_ref(&name, !self.type_name.array_bounds.is_empty())
        });
        let default = self
            .default
            .as_ref()
            .map(|expr| expr.resolve_in(context, &ScalarAnalysisContext::default()))
            .transpose()?;
        Ok(self.mode.resolve_parameter(&name, &param_type, &default))
    }
}

impl ParameterMode {
    const fn is_output(self) -> bool {
        matches!(self, Self::Out | Self::InOut | Self::Table)
    }

    fn resolve_parameter(
        self,
        name: &Option<String>,
        param_type: &Option<TypeReference>,
        default: &Option<SemScalarExpr>,
    ) -> FunctionParam {
        match self {
            Self::In => FunctionParam::new_input_param(name, param_type, default),
            Self::Out => FunctionParam::new_output_param(name, param_type),
            Self::InOut => FunctionParam::new_in_out_param(name, param_type, default),
            Self::Variadic => FunctionParam::new_variadig_param(name, param_type),
            Self::Table => FunctionParam::new_table_param(name, param_type),
        }
    }
}

impl Function {
    pub(super) const fn new(
        name: Name,
        parameters: Vec<FunctionParameter>,
        result_type: Option<TypeName>,
        options: Vec<FunctionOption>,
        replace: bool,
        procedure: bool,
    ) -> Self {
        Self {
            name,
            parameters,
            result_type,
            options,
            replace,
            procedure,
            body: None,
        }
    }

    pub(super) fn with_body(mut self, body: Option<FunctionBody>) -> Self {
        self.body = body;
        self
    }

    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateFunction, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }

    fn resolve_in(&self, context: &mut impl ParseContext) -> Result<CreateFunction, AnalysisError> {
        let name = self.name.full_name()?;
        if self.procedure || self.result_type.is_none() {
            let mut params = Vec::new();
            let mut outputs = Vec::new();
            for param in &self.parameters {
                let resolved = param.resolve(context)?;
                if param.mode.is_output() {
                    outputs.push(resolved.get_type().cloned());
                }
                params.push(resolved);
            }
            let ret_type = if self.procedure {
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
            let strict = self.options.iter().any(FunctionOption::is_strict);
            let overload = OverloadVariant::new(&ret_type, &params, strict, false);
            return Ok(CreateFunction::new(
                &name,
                &FunctionOverloadCollection::new(&[overload]),
                &None,
            )
            .with_procedure_kind(self.procedure));
        }
        let Some(result_type) = &self.result_type else {
            return AnalysisError::raise_unexpected_none("create_function.return_type");
        };
        let ret_name = result_type.name.full_name()?;
        let returning = if result_type.set_of {
            if ret_name.name().as_str() == "record" {
                let mut columns = Vec::new();
                for param in &self.parameters {
                    if param.mode.is_output() {
                        let ty = TypeReference::from_full_name(param.type_name.name.full_name()?);
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
        for param in &self.parameters {
            params.push(param.resolve(context)?);
        }
        let ret_type = if let Some(ty) = context.get_type(&ret_name) {
            ty.type_reference().cloned()
        } else {
            None
        };
        let strict = self.options.iter().any(FunctionOption::is_strict);
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
        let function = self.resolve(&mut context)?.with_replace(self.replace);
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

#[cfg(test)]
mod test_function_null_input_semantics {
    use super::Function;
    use crate::ast::TryFromWithTokens;
    use crate::syn::Opt;
    use crate::test_helpers::{SynBuilder, TestParseContext};
    use sql_fun_core::IVec;

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
        let resolved = function.resolve(&mut context).unwrap();
        assert_eq!(
            expected,
            resolved.overloads().get_at(0).unwrap().is_strict()
        );
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Domain {
    name: Name,
    base: TypeName,
    base_span: StringSpan,
    constraints: Vec<super::TableConstraint>,
    collation: Name,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositeType {
    name: Name,
    fields: Vec<CompositeField>,
}

#[derive(Clone, Debug)]
pub(crate) struct CompositeField {
    name: String,
    type_name: TypeName,
    collation: Option<Name>,
    is_not_null: bool,
}

impl CompositeType {
    pub(super) fn new(name: &Name, fields: &[CompositeField]) -> Self {
        Self {
            name: name.clone(),
            fields: fields.to_vec(),
        }
    }

    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateCompositeType, AnalysisError> {
        let mut fields = CompositeFieldCollection::default();
        for field in &self.fields {
            fields.push(field.resolve(context)?);
        }
        Ok(CreateCompositeType::new(&self.name.full_name()?, &fields))
    }

    pub(super) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let composite = self.resolve(&mut context)?;
        let context = context.apply_create_comp_type(&composite)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateCompType(composite),
            context,
        ))
    }
}

impl CompositeField {
    pub(super) fn new(
        name: &str,
        type_name: &TypeName,
        collation: Option<&Name>,
        is_not_null: bool,
    ) -> Self {
        Self {
            name: name.to_owned(),
            type_name: type_name.clone(),
            collation: collation.cloned(),
            is_not_null,
        }
    }

    fn resolve(
        &self,
        _context: &mut impl ParseContext,
    ) -> Result<CompositeFieldDefinition, AnalysisError> {
        let type_ref = self.type_name.reference()?;
        Ok(CompositeFieldDefinition::new(
            &self.name,
            &Some(type_ref),
            &Some(self.is_not_null),
        ))
    }
}

#[cfg(test)]
mod test_composite_field_resolution {
    use super::CompositeType;
    use crate::ast::TryFromWithTokens;
    use crate::sem::{PgBuiltInType, TypeReference};
    use crate::syn::Opt;
    use crate::test_helpers::{SynBuilder, TestParseContext};
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
        let field = composite.fields[0]
            .resolve(&mut TestParseContext::default())
            .unwrap();
        assert_eq!(field.name(), "items");
        assert_eq!(
            field.get_type(),
            Some(TypeReference::concrete_type_ref(
                PgBuiltInType::int4().full_name(),
                is_array,
            ))
        );
    }
}

impl Domain {
    pub(super) const fn new(
        name: Name,
        base: TypeName,
        base_span: StringSpan,
        constraints: Vec<super::TableConstraint>,
        collation: Name,
    ) -> Self {
        Self {
            name,
            base,
            base_span,
            constraints,
            collation,
        }
    }

    pub(super) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<CreateDomain, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context);
        context.finish_analysis(result)
    }

    fn resolve_in(&self, context: &mut impl ParseContext) -> Result<CreateDomain, AnalysisError> {
        let name = self.name.full_name()?;
        let base_name = self.base.name.full_name()?;
        let base = context.canonicalize_type_ref(&self.base.reference()?);
        if base.is_none() {
            context.report_analysis_problem(AnalysisProblem::domain_base_type_not_found(
                &name,
                &base_name.to_string(),
                &self.base_span,
            ))?;
        }
        let mut domain = CreateDomain::new(&TypeReference::from_full_name(name), &base);
        let category = if let Some(base) = context.get_type(&base_name) {
            if self.base.array_bounds.is_empty() {
                base.category(context)
            } else if context.dialect() == crate::ast::Dialect::postgres() {
                TypeCategory::try_from('A').ok()
            } else {
                None
            }
        } else {
            None
        };
        domain.set_category(category);
        Ok(domain)
    }

    pub(crate) fn resolve_and_apply<TContext: ParseContext>(
        &self,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let domain = self.resolve(&mut context)?;
        let context = context.apply_create_domain(&domain)?;
        Ok(AstAndContextPair::new(
            SemAst::create_domain(&domain),
            context,
        ))
    }
}

#[cfg(test)]
mod domain_tests {
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
        let domain = syntax.resolve(&mut context)?;
        assert!(matches!(domain, SemAst::CreateDomain(_)));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl super::AlterRelation {
    fn view_default_changes(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterView>, ResolutionFailure> {
        let requested = self.name.full_name()?;
        let Some(name) = expand_relation_search_path(context, &requested, |key| {
            context
                .get_view_impl(key)
                .filter(|view| !view.is_materialized())
                .map(|_| key.clone())
        }) else {
            return Ok(None);
        };
        let mut actions = Vec::new();
        for command in &self.commands {
            let Some(action) = command.view_default_action(context)? else {
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
impl super::AlterCommand {
    fn view_default_action(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterObjSubCommand>, ResolutionFailure> {
        match self {
            Self::Postgres(command) => command.view_default_action(context),
            Self::Default(name, expression) => {
                let value = expression.resolve(
                    context,
                    &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
                )?;
                Ok(Some(AlterObjSubCommand::column_default(
                    &ColumnName::from(name.as_str()),
                    &value,
                )))
            }
            _ => Ok(None),
        }
    }
}
impl PostgresAlterCommand {
    fn view_default_action(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<AlterObjSubCommand>, ResolutionFailure> {
        let action = match self.operation {
            AlterOperation::ColumnDefault => {
                let value = self.definition.view_default(context)?;
                match value {
                    Some(value) => AlterObjSubCommand::column_default(
                        &ColumnName::from(self.name.as_str()),
                        &value,
                    ),
                    None => AlterObjSubCommand::drop_column_default(&ColumnName::from(
                        self.name.as_str(),
                    )),
                }
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
                return Err(ResolutionFailure::view(&ViewProblem::invalid_alter()));
            }
            _ => return Ok(None),
        };
        Ok(Some(action))
    }
}
impl AlterDefinition {
    fn view_default(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, ResolutionFailure> {
        match self {
            Self::None => Ok(None),
            Self::Expression(value) => value
                .resolve(
                    context,
                    &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
                )
                .map(Some)
                .map_err(Into::into),
            _ => {
                AnalysisError::raise_unexpected_input("view default expression").map_err(Into::into)
            }
        }
    }
}
