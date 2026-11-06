mod ddl;

use crate::{
    context::GetSourcePathWithSpan,
    names::{ColumnName, TableName, ViewName},
    sem::{
        AlterColumnAttributes, AnalysisError, AnalysisProblem, ColumnDefinition, ColumnRename,
        Constraint, ConstraintNames, CreateTable, CreateView, OwnerSpecification, ParseContext,
        SemScalarExpr, SourcePathWithSpan, problem::ViewProblem,
    },
};

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
