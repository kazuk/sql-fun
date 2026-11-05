mod ddl;

#[cfg(feature = "sqlite")]
use crate::{
    SqliteToken,
    syn::sqlite::{AlterTable as SqliteAlterTable, AlterTableBody},
};
use crate::{
    ast::{AlterCommand, AlterRelation, TryFromWithTokens},
    context::GetSourcePathWithSpan,
    names::{ColumnName, TableName, ViewName},
    sem::{
        AlterColumnAttributes, AnalysisError, AnalysisProblem, AstAndContextPair, ColumnDefinition,
        ColumnRename, Constraint, ConstraintNames, CreateTable, CreateView, OwnerSpecification,
        ParseContext, RelationAlteration, SemScalarExpr, SourcePathWithSpan, problem::ViewProblem,
    },
    syn::{AlterTableCmd, AlterTableStmt, AlterTableType, ScanToken},
};
use sql_fun_core::IVec;

/// analyzed alter table statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterTable {
    table: TableName,
    commands: Vec<AlterObjSubCommand>,
    source_span: SourcePathWithSpan,
    #[getter(skip)]
    applicable: bool,
    #[getter(skip)]
    resolved_target: Option<TableName>,
}

impl GetSourcePathWithSpan for AlterTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source_span
    }
}

impl AlterTable {
    pub(crate) fn column_rename(
        table: &TableName,
        target: Option<&TableName>,
        rename: &ColumnRename,
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(
            table,
            &[AlterObjSubCommand::analyzed_column_rename(rename)],
            source,
        );
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some() && rename.can_apply();
        statement
    }
    /// Completed standalone column rename, including rejected operations.
    pub fn column_rename_change(&self) -> Option<&ColumnRename> {
        let [command] = self.commands.as_slice() else {
            return None;
        };
        command.column_rename_change()
    }

    /// Whether completed semantic analysis permits catalog application.
    pub const fn can_apply(&self) -> bool {
        self.applicable
    }
    /// Canonical target binding retained by semantic analysis.
    pub const fn resolved_target(&self) -> Option<&TableName> {
        self.resolved_target.as_ref()
    }
    pub(crate) fn table_rename(
        table: &TableName,
        target: Option<&TableName>,
        destination: &TableName,
        permitted: bool,
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(table, &[AlterObjSubCommand::rename_to(destination)], source);
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some() && permitted;
        statement
    }

    /// Destination identity retained for a standalone table rename.
    pub const fn rename_destination(&self) -> Option<&TableName> {
        let [command] = self.commands.as_slice() else {
            return None;
        };
        command.rename_destination()
    }

    pub(crate) fn column_attributes(
        table: &TableName,
        target: Option<&TableName>,
        commands: &[AlterObjSubCommand],
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(table, commands, source);
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some()
            && commands
                .iter()
                .all(AlterObjSubCommand::can_apply_column_attributes);
        statement
    }
    pub(crate) fn validate_check_names(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut names = ConstraintNames::default();
        let mut reported = ConstraintNames::default();
        for command in &self.commands {
            if let Some(constraint) = command.added_constraint() {
                let name = constraint.name();
                names.reserve(name);
            }
        }
        for command in &mut self.commands {
            if let AlterObjSubCommand::AddConstraint(Constraint::Check(check)) = command
                && names.is_duplicate(check.name())
                && reported.reserve(check.name())
            {
                check.reject_duplicate_name(context)?;
                self.applicable = false;
            }
        }
        Ok(())
    }

    pub(crate) fn conflicting_constraint(&self, table: &CreateTable) -> Option<String> {
        let mut names = ConstraintNames::default();
        for command in &self.commands {
            if let Some(constraint) = command.added_constraint() {
                let name = constraint.name();
                if !name.is_empty() && (table.constraint(name).is_some() || !names.reserve(name)) {
                    return Some(name.to_owned());
                }
            }
        }
        None
    }

    pub(crate) fn applied_owner(&self) -> Option<&OwnerSpecification> {
        self.commands
            .iter()
            .rev()
            .find_map(AlterObjSubCommand::applied_owner)
    }

    pub(crate) fn new(
        table: &TableName,
        commands: &[AlterObjSubCommand],
        source_span: &SourcePathWithSpan,
    ) -> Self {
        Self {
            table: table.clone(),
            commands: commands.to_vec(),
            source_span: source_span.clone(),
            applicable: commands
                .iter()
                .all(AlterObjSubCommand::declaration_can_apply),
            resolved_target: None,
        }
    }
    /// analyze [`crate::syn::AlterTableStmt`]
    pub fn analyze_alter_table_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        RelationAlteration::from_ast_and_apply_at(
            &AlterRelation::try_from_with_tokens(&syn, tokens)?,
            context,
            source_span,
        )
    }

    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        RelationAlteration::from_ast_and_apply_at(
            &AlterRelation::try_from_with_tokens(&syn, tokens)?,
            context,
            source_span,
        )
    }

    /// analyze [`crate::syn::sqlite::AlterTable`]
    #[cfg(feature = "sqlite")]
    #[tracing::instrument(skip(context, syn))]
    pub fn analyze_alter_table<TParseContext>(
        context: TParseContext,
        syn: SqliteAlterTable,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        RelationAlteration::from_ast_and_apply_at(
            &AlterRelation::try_from(&syn)?,
            context,
            source_span,
        )
    }
}

/// analyzed alter view statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterView {
    view: ViewName,
    commands: Vec<AlterObjSubCommand>,
}

impl AlterView {
    pub(crate) fn new(view: &ViewName, commands: &[AlterObjSubCommand]) -> Self {
        Self {
            view: view.clone(),
            commands: commands.to_vec(),
        }
    }
    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_view_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        as_matview: bool,
        syn: AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        RelationAlteration::from_ast_and_apply(
            &AlterRelation::try_from_with_tokens(&syn, &IVec::default())?
                .with_view_kind(as_matview),
            context,
        )
    }
}

#[derive(Debug, Clone)]
pub enum AlterObjSubCommand {
    /// An analyzed PostgreSQL column-attribute operation.
    ColumnAttributes(Box<AlterColumnAttributes>),
    ChangeOwner(OwnerSpecification),
    SetColumnDefault(ColumnName, Box<SemScalarExpr>),
    /// Remove a column default.
    DropColumnDefault(ColumnName),
    AddConstraint(Constraint),
    ClusterOn(String),

    RenameTo(TableName),
    AddColumn(ColumnDefinition),
    AlterColumn(ColumnName, ColumnDefinition),
    RenameColumn(Box<ColumnRename>),
    DropColumn(ColumnName),
}

impl AlterObjSubCommand {
    fn declaration_can_apply(&self) -> bool {
        match self {
            Self::AddConstraint(constraint) => {
                constraint.as_check().is_none_or(|check| check.can_apply())
            }
            Self::RenameColumn(rename) => rename.can_apply(),
            Self::AddColumn(column) | Self::AlterColumn(_, column) => {
                column.declaration_can_apply()
            }
            _ => true,
        }
    }

    const fn rename_destination(&self) -> Option<&TableName> {
        if let Self::RenameTo(name) = self {
            Some(name)
        } else {
            None
        }
    }

    pub(crate) fn column_attributes(value: &AlterColumnAttributes) -> Self {
        Self::ColumnAttributes(Box::new(value.clone()))
    }
    /// Retained column binding, requested action, and validation outcomes.
    pub fn column_attribute_change(&self) -> Option<&AlterColumnAttributes> {
        if let Self::ColumnAttributes(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn can_apply_column_attributes(&self) -> bool {
        self.column_attribute_change()
            .is_some_and(AlterColumnAttributes::can_apply)
    }
    pub(crate) fn apply_column_attributes(&self, table: &mut CreateTable) {
        if let Self::ColumnAttributes(value) = self {
            value.apply(table);
        }
    }
    const fn added_constraint(&self) -> Option<&Constraint> {
        if let Self::AddConstraint(constraint) = self {
            Some(constraint)
        } else {
            None
        }
    }

    const fn applied_owner(&self) -> Option<&OwnerSpecification> {
        if let Self::ChangeOwner(owner) = self
            && owner.can_apply()
        {
            Some(owner)
        } else {
            None
        }
    }

    pub(crate) fn resolved_owner(owner: &OwnerSpecification) -> Self {
        Self::ChangeOwner(owner.clone())
    }
    pub(crate) fn drop_column_default(column: &ColumnName) -> Self {
        Self::DropColumnDefault(column.clone())
    }
    pub(crate) fn column_default(column: &ColumnName, expression: &SemScalarExpr) -> Self {
        Self::SetColumnDefault(column.clone(), Box::new(expression.clone()))
    }
    pub(crate) fn add_constraint(constraint: &Constraint) -> Self {
        Self::AddConstraint(constraint.clone())
    }
    pub(crate) fn cluster_on(index: &str) -> Self {
        Self::ClusterOn(index.to_owned())
    }
    pub(crate) fn rename_to(table: &TableName) -> Self {
        Self::RenameTo(table.clone())
    }
    pub(crate) fn add_column(column: &ColumnDefinition) -> Self {
        Self::AddColumn(column.clone())
    }
    pub(crate) fn alter_column(name: &ColumnName, column: &ColumnDefinition) -> Self {
        Self::AlterColumn(name.clone(), column.clone())
    }
    pub(crate) fn analyzed_column_rename(rename: &ColumnRename) -> Self {
        Self::RenameColumn(Box::new(rename.clone()))
    }
    /// Completed column-rename semantics.
    pub fn column_rename_change(&self) -> Option<&ColumnRename> {
        if let Self::RenameColumn(rename) = self {
            Some(rename)
        } else {
            None
        }
    }
    pub(crate) fn drop_column(column: &ColumnName) -> Self {
        Self::DropColumn(column.clone())
    }
}

#[cfg(test)]
pub fn analyze_alter_obj_change_owner<TParseContext>(
    _context: &TParseContext,
    cmd: AlterTableCmd,
) -> Result<AlterObjSubCommand, AnalysisError> {
    let Some(new_owner) = cmd.get_newowner().get_rolename() else {
        AnalysisError::raise_unexpected_none("alter_table_cmd.newwoner")?
    };
    Ok(AlterObjSubCommand::resolved_owner(
        &OwnerSpecification::named(&new_owner),
    ))
}

impl AlterObjSubCommand {
    #[cfg(test)]
    fn analyze_alter_obj_column_default<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        cmd: AlterTableCmd,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_report(
            &AlterCommand::try_from_with_tokens(&cmd, tokens)?,
            context,
            table_name,
        )
    }
}

#[cfg(feature = "sqlite")]
impl AlterObjSubCommand {
    pub fn analyze_alter_table_body<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        body: &AlterTableBody,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_report(&AlterCommand::try_from(body)?, context, table_name)
    }
}

impl AlterObjSubCommand {
    pub fn analyze_alter_table_cmd<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        cmd: AlterTableCmd,
        _subtyp: AlterTableType,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_report(
            &AlterCommand::try_from_with_tokens(&cmd, tokens)?,
            context,
            table_name,
        )
    }
}

impl AlterObjSubCommand {
    pub(crate) fn validate_for_view(
        &self,
        context: &mut impl ParseContext,
        view: &CreateView,
    ) -> Result<bool, AnalysisError> {
        let name = match self {
            Self::SetColumnDefault(name, _) | Self::DropColumnDefault(name) => name,
            _ => return Ok(true),
        };
        if view.has_column(name) {
            return Ok(true);
        }
        context.report_analysis_problem(AnalysisProblem::view(
            &ViewProblem::unknown_default_column(view.name(), name.as_str()),
        ))?;
        Ok(false)
    }
    pub(crate) fn apply_to_view(&self, view: &mut CreateView) {
        match self {
            Self::ChangeOwner(owner) => {
                if owner.can_apply() {
                    view.change_owner(owner);
                }
            }
            Self::SetColumnDefault(name, value) => view
                .change_column_default(name, Some(value))
                .expect("validated default column"),
            Self::DropColumnDefault(name) => view
                .change_column_default(name, None)
                .expect("validated default column"),
            _ => todo!(),
        }
    }
}
