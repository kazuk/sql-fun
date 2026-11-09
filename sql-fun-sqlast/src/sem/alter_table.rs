mod ddl;

use crate::{
    AstAndContextPair,
    ast::{Statement, StatementNode},
    context::GetSourcePathWithSpan,
    names::{ColumnName, TableName, ViewName},
    sem::{
        AlterColumnAttributes, AlterColumnDefinition, AlterColumnType, AnalysisError,
        AnalysisProblem, ColumnAddition, ColumnDefinition, ColumnDrop, ColumnRename, Constraint,
        ConstraintNames, CreateIndex, CreateSequence, CreateTable, CreateView, OwnerSpecification,
        ParseContext, RelationAlteration, SemScalarExpr, SourcePathWithSpan, problem::ViewProblem,
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
    #[getter(skip)]
    addition_indexes: Option<Vec<CreateIndex>>,
    #[getter(skip)]
    addition_sequences: Vec<CreateSequence>,
}

impl GetSourcePathWithSpan for AlterTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source_span
    }
}

impl AlterTable {
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn column_additions(
        requested: &TableName,
        target: Option<&TableName>,
        commands: &[AlterObjSubCommand],
        source: &SourcePathWithSpan,
        applicable: bool,
        indexes: &[CreateIndex],
        sequences: &[CreateSequence],
    ) -> Self {
        let mut statement = Self::new(requested, commands, source);
        statement.resolved_target = target.cloned();
        statement.applicable &= target.is_some() && applicable;
        statement.addition_indexes = Some(indexes.to_vec());
        statement.addition_sequences = sequences.to_vec();
        statement
    }

    pub(crate) fn addition_indexes(&self) -> Option<&[CreateIndex]> {
        self.addition_indexes.as_deref()
    }
    pub(crate) fn addition_sequences(&self) -> &[CreateSequence] {
        &self.addition_sequences
    }

    /// Analyze an ALTER TABLE without applying it to the schema catalog.
    ///
    /// The context supplies the selected engine, catalog, diagnostics and source
    /// association. The returned statement retains semantic applicability;
    /// applicability does not imply that catalog application is implemented.
    /// Other statement kinds are rejected as an invalid API input.
    pub fn analyze<TContext: ParseContext>(
        statement: &Statement,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let StatementNode::AlterRelation(relation) = statement.node() else {
            return AnalysisError::raise_unexpected_input("expected ALTER TABLE");
        };
        if !relation.kind().supports_table_column_attributes() {
            return AnalysisError::raise_unexpected_input("expected ALTER TABLE relation kind");
        }
        if statement.dialect() != context.dialect() {
            return AnalysisError::raise_unexpected_input("statement and context dialects differ");
        }
        let source = context.get_source_path_with_span().cloned();
        context.begin_parameter_type_analysis();
        let result = RelationAlteration::from_ast_inner(relation, &mut context, source.as_ref());
        let analyzed = context.finish_analysis(result)?;
        Ok(AstAndContextPair::new(analyzed, context))
    }
}

#[cfg(test)]
mod analysis_only_tests {
    use crate::{
        ast::Dialect,
        context::BaseParseContext,
        sem::AlterTable,
        test_helpers::{column_attributes, public_analysis},
    };

    #[rstest::rstest]
    #[case::applicable("ALTER TABLE t ALTER COLUMN a SET NOT NULL", true)]
    #[case::rejected("ALTER TABLE t ALTER COLUMN missing SET NOT NULL", false)]
    fn analysis_retains_outcomes_without_catalog_application(
        #[case] sql: &str,
        #[case] applicable: bool,
    ) -> testresult::TestResult {
        let context = column_attributes::seed("CREATE TABLE t(a integer)", "17")?;
        let statement = public_analysis::statement(sql, Dialect::postgres());
        let result = AlterTable::analyze(&statement, context)?;
        assert_eq!(
            result.ast().alter_table_statement().unwrap().can_apply(),
            applicable
        );
        let table = result
            .context()
            .get_table(&column_attributes::table_name())
            .unwrap();
        assert_eq!(
            column_attributes::column(result.context(), "a").is_not_null(),
            Some(false)
        );
        assert!(table.alter_statements().is_empty());
        Ok(())
    }
}

impl AlterTable {
    pub(crate) fn analyzed_drops(
        table: &TableName,
        target: Option<&TableName>,
        commands: &[AlterObjSubCommand],
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut result = Self::new(table, commands, source);
        result.resolved_target = target.cloned();
        result
    }
    pub(crate) fn analyzed_column_types(
        table: &TableName,
        target: Option<&TableName>,
        commands: &[AlterObjSubCommand],
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut result = Self::new(table, commands, source);
        result.resolved_target = target.cloned();
        result.applicable = target.is_some()
            && commands.iter().all(|command| {
                if let Some(change) = command.column_type_change() {
                    change.is_valid()
                } else if let Some(change) = command.column_definition_change() {
                    change.is_valid()
                } else {
                    command.can_apply_column_attributes()
                }
            });
        result
    }
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

    /// Whether completed analysis found no rejection that prevents application.
    ///
    /// Column type changes and removals still require final dependency validation
    /// by schema application; true is not a guarantee of successful application.
    pub const fn can_apply(&self) -> bool {
        self.applicable
    }
    /// Whether final dependency validation remains with schema application.
    pub fn requires_application_validation(&self) -> bool {
        self.commands.iter().any(|command| {
            command.column_type_change().is_some() || command.column_drop().is_some()
        })
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
            addition_indexes: None,
            addition_sequences: Vec::new(),
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
    /// Analyzed column type conversion, without catalog application.
    ColumnType(Box<AlterColumnType>),
    /// Parser-retained SQLite column definition replacement and its engine rejection.
    ColumnDefinition(Box<AlterColumnDefinition>),
    ChangeOwner(OwnerSpecification),
    SetColumnDefault(ColumnName, Box<SemScalarExpr>),
    /// Remove a column default.
    DropColumnDefault(ColumnName),
    AddConstraint(Constraint),
    ClusterOn(String),

    RenameTo(TableName),
    AddColumn(Box<ColumnAddition>),
    AlterColumn(ColumnName, ColumnDefinition),
    RenameColumn(Box<ColumnRename>),
    /// Analyzed removal, without schema-catalog application.
    DropColumn(Box<ColumnDrop>),
}

impl AlterObjSubCommand {
    fn declaration_can_apply(&self) -> bool {
        match self {
            Self::ColumnAttributes(change) => change.can_apply(),
            Self::ColumnType(change) => change.is_valid(),
            Self::DropColumn(change) => change.is_locally_applicable(),
            Self::ColumnDefinition(change) => change.is_valid(),
            Self::AddConstraint(constraint) => {
                constraint.as_check().is_none_or(|check| check.can_apply())
            }
            Self::RenameColumn(rename) => rename.can_apply(),
            Self::AddColumn(addition) => addition.can_apply(),
            Self::AlterColumn(_, column) => column.declaration_can_apply(),
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

    pub(crate) fn column_definition(value: &AlterColumnDefinition) -> Self {
        Self::ColumnDefinition(Box::new(value.clone()))
    }
    /// Completed parser-retained SQLite replacement analysis, including rejection.
    pub fn column_definition_change(&self) -> Option<&AlterColumnDefinition> {
        if let Self::ColumnDefinition(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn column_type(value: &AlterColumnType) -> Self {
        Self::ColumnType(Box::new(value.clone()))
    }
    /// Completed column type analysis, including invalid operations.
    pub fn column_type_change(&self) -> Option<&AlterColumnType> {
        if let Self::ColumnType(value) = self {
            Some(value)
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
    pub(crate) fn addition_indexes(
        &self,
        table: &TableName,
    ) -> Result<Vec<CreateIndex>, AnalysisError> {
        if let Some(constraint) = self.added_constraint() {
            constraint.addition_indexes(table)
        } else {
            Ok(Vec::new())
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
    pub(crate) fn add_column(addition: &ColumnAddition) -> Self {
        Self::AddColumn(Box::new(addition.clone()))
    }
    /// Completed ADD COLUMN declaration, including skipped and rejected requests.
    pub fn column_addition(&self) -> Option<&ColumnAddition> {
        if let Self::AddColumn(addition) = self {
            Some(addition)
        } else {
            None
        }
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
    pub(crate) fn drop_column(change: &ColumnDrop) -> Self {
        Self::DropColumn(Box::new(change.clone()))
    }
    /// Completed DROP COLUMN analysis, including skipped and rejected requests.
    pub fn column_drop(&self) -> Option<&ColumnDrop> {
        if let Self::DropColumn(change) = self {
            Some(change)
        } else {
            None
        }
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
