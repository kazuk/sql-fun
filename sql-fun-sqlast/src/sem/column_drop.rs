//! Completed DROP analysis and dependency knowledge, without catalog publication.

mod dependencies;

use crate::{
    ast::{AlterCommand as AstCommand, AlterRelation as AstRelation, Dialect, ddl::AlterOperation},
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, ColumnAttributeAction,
        ColumnDefinition, CreateTable, SemAst, SourcePathWithSpan, problem::AlterColumnProblem,
    },
};

/// Completed local outcome; application may still reject an applicable operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnDropOutcome {
    /// Target and known semantic checks permit removal.
    Applicable,
    /// An explicit IF EXISTS skips the missing relation or column.
    Skipped,
    /// A completed lookup or semantic check rejects removal.
    Rejected,
}
impl ColumnDropOutcome {
    const fn applicable() -> Self {
        Self::Applicable
    }
    const fn skipped() -> Self {
        Self::Skipped
    }
    const fn rejected() -> Self {
        Self::Rejected
    }
}

/// Kind of a known dependent semantic object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropDependencyKind {
    /// A named or unnamed table/column constraint.
    Constraint,
    /// A retained index definition.
    Index,
    /// An ordinary or materialized view.
    View,
    /// A generated column.
    GeneratedColumn,
    /// A trigger definition.
    Trigger,
    /// A rule definition.
    Rule,
}
impl DropDependencyKind {
    pub(crate) const fn constraint() -> Self {
        Self::Constraint
    }
    pub(crate) const fn index() -> Self {
        Self::Index
    }
    pub(crate) const fn view() -> Self {
        Self::View
    }
    pub(crate) const fn generated_column() -> Self {
        Self::GeneratedColumn
    }
    pub(crate) const fn trigger() -> Self {
        Self::Trigger
    }
    pub(crate) const fn rule() -> Self {
        Self::Rule
    }
}
/// What the selected dialect requires for a known dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropDependencyDecision {
    /// PostgreSQL automatically removes the dependent object.
    Automatic,
    /// The explicit CASCADE permits removal; application owns its full closure.
    Cascade,
    /// SQLite wildcard outputs need application-time reconstruction, not rejection.
    Revalidate,
    /// The dependency prevents this requested operation.
    Reject,
}
/// A dependency identity and completed local decision, for downstream application.
#[derive(Clone, Debug)]
pub struct DropDependency {
    relation: FullName,
    object: Option<FullName>,
    member: String,
    kind: DropDependencyKind,
    decision: DropDependencyDecision,
}
impl DropDependency {
    fn same_object(&self, other: &Self) -> bool {
        self.relation == other.relation
            && self.object == other.object
            && self.member == other.member
            && self.kind == other.kind
    }
    const fn removes_object(&self) -> bool {
        matches!(
            self.decision,
            DropDependencyDecision::Automatic | DropDependencyDecision::Cascade
        )
    }
    /// Canonical relation owning the dependent object, or the view's identity.
    pub const fn relation(&self) -> &FullName {
        &self.relation
    }
    /// Canonical standalone object identity, including a trigger's own schema.
    pub const fn object(&self) -> Option<&FullName> {
        self.object.as_ref()
    }
    /// Local object/column identity. Table constraints use their retained catalog key;
    /// anonymous inline constraints use `column#index` within the captured definition.
    pub fn member(&self) -> &str {
        &self.member
    }
    /// Semantic object kind.
    pub const fn kind(&self) -> DropDependencyKind {
        self.kind
    }
    /// Recorded dialect-specific dependency decision.
    pub const fn decision(&self) -> DropDependencyDecision {
        self.decision
    }
}
/// Boundaries of dependency knowledge, never evidence that dependencies are absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropKnowledgeGap {
    /// Column-specific sequence ownership is not represented (#545).
    SequenceOwnership,
    /// Some expression dependencies have no completed column binding (#1062).
    UnboundExpression,
    /// Full dependency closure and final validation belong to #1076.
    ApplicationClosure,
}

/// An analyzed column removal. Local applicability does not guarantee application.
#[derive(Clone, Debug)]
pub struct ColumnDrop {
    table: TableName,
    requested: ColumnName,
    original: Option<ColumnDefinition>,
    if_exists: bool,
    cascade: bool,
    sqlite: bool,
    only: bool,
    relation_if_exists: bool,
    outcome: ColumnDropOutcome,
    dependencies: Vec<DropDependency>,
    knowledge_gaps: Vec<DropKnowledgeGap>,
    problems: Vec<AlterColumnProblem>,
}
impl ColumnDrop {
    /// Bound table identity; the requested name if lookup failed.
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// Column spelling supplied by the statement.
    pub const fn requested(&self) -> &ColumnName {
        &self.requested
    }
    /// Original definition captured from the completed column lookup.
    pub const fn original(&self) -> Option<&ColumnDefinition> {
        self.original.as_ref()
    }
    /// Whether ONLY was explicitly requested for the relation.
    pub const fn only(&self) -> bool {
        self.only
    }
    /// Whether relation-level IF EXISTS was explicitly requested.
    pub const fn relation_if_exists(&self) -> bool {
        self.relation_if_exists
    }
    /// Whether column-level IF EXISTS was explicitly requested.
    pub const fn if_exists(&self) -> bool {
        self.if_exists
    }
    /// Whether CASCADE was explicitly requested.
    pub const fn cascade(&self) -> bool {
        self.cascade
    }
    /// Completed skipped/rejected/local-applicability outcome.
    pub const fn outcome(&self) -> ColumnDropOutcome {
        self.outcome
    }
    /// Whether no known local semantic rejection prevents removal (including skips).
    pub fn is_locally_applicable(&self) -> bool {
        self.outcome != ColumnDropOutcome::rejected()
    }
    /// Known dependent object identities and decisions; not a complete application closure.
    pub fn dependencies(&self) -> &[DropDependency] {
        &self.dependencies
    }
    /// Explicit limits of dependency analysis.
    pub fn knowledge_gaps(&self) -> &[DropKnowledgeGap] {
        &self.knowledge_gaps
    }
    /// Structured semantic reasons, retained even when diagnostics are suppressed.
    pub fn problems(&self) -> &[AlterColumnProblem] {
        &self.problems
    }

    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        table: &TableName,
        definition: Option<&CreateTable>,
        column: &ColumnName,
        if_exists: bool,
        cascade: bool,
        skipped_relation: bool,
    ) -> Result<Self, AnalysisError> {
        let sqlite = context.dialect() == Dialect::sqlite();
        let original = if let Some(table) = definition {
            table.find_column(column, sqlite).cloned()
        } else {
            None
        };
        let mut change = Self {
            table: table.clone(),
            requested: column.clone(),
            original,
            if_exists,
            cascade,
            sqlite,
            only: false,
            relation_if_exists: false,
            outcome: ColumnDropOutcome::applicable(),
            dependencies: Vec::new(),
            knowledge_gaps: vec![
                DropKnowledgeGap::SequenceOwnership,
                DropKnowledgeGap::ApplicationClosure,
            ],
            problems: Vec::new(),
        };
        if skipped_relation || (definition.is_some() && change.original.is_none() && if_exists) {
            change.outcome = ColumnDropOutcome::skipped();
        } else if definition.is_none() {
            change.reject(AlterColumnProblem::missing_relation());
        } else if change.original.is_none() {
            change.reject(AlterColumnProblem::missing_column());
        } else if let Some(definition) = definition {
            if sqlite && !definition.has_non_generated_column_after_drop(&change) {
                change.reject(AlterColumnProblem::drop_last_column());
            }
            change.analyze_dependencies(context, definition)?;
        }
        Ok(change)
    }
    pub(crate) fn finish(
        &mut self,
        context: &mut impl ParseContext,
        retired: &[DropDependency],
    ) -> Result<(), AnalysisError> {
        self.dependencies.retain(|dependency| {
            !retired
                .iter()
                .any(|previous| previous.same_object(dependency))
        });
        for dependency in &self.dependencies {
            if dependency.decision == DropDependencyDecision::Reject {
                self.problems.push(AlterColumnProblem::drop_dependency(
                    &dependency.relation,
                    &dependency.member,
                ));
                self.outcome = ColumnDropOutcome::rejected();
            }
        }
        for problem in &self.problems {
            context.report_problem(AnalysisProblem::alter_column(
                &self.table,
                &self.requested,
                problem,
            ))?;
        }
        Ok(())
    }
    fn stage(&self, table: &mut CreateTable, retired: &mut Vec<DropDependency>) {
        if self.outcome != ColumnDropOutcome::applicable() {
            return;
        }
        if let Some(original) = &self.original
            && let Some(name) = original.name()
        {
            table.stage_column_removal(name);
        }
        for dependency in &self.dependencies {
            if dependency.removes_object() {
                if dependency.kind == DropDependencyKind::GeneratedColumn
                    && dependency.relation == *self.table.full_name()
                {
                    table.stage_column_removal(&ColumnName::from(dependency.member.as_str()));
                }
                retired.push(dependency.clone());
            }
        }
    }
    fn reject(&mut self, problem: AlterColumnProblem) {
        self.outcome = ColumnDropOutcome::rejected();
        self.problems.push(problem);
    }
    fn syntax(command: &AstCommand) -> Option<(&str, bool, bool)> {
        match command {
            AstCommand::DropColumn(name) => Some((name, false, false)),
            AstCommand::Postgres(command)
                if matches!(command.operation(), AlterOperation::DropColumn) =>
            {
                Some((command.name(), command.missing_ok(), command.cascade()))
            }
            _ => None,
        }
    }
    pub(crate) fn is_ast_relation(ast: &AstRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| Self::syntax(command).is_some())
    }
    pub(crate) fn analyze_relation(
        ast: &AstRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let source = source.ok_or_else(|| {
            AnalysisError::unexpected_input_error("DROP COLUMN source association")
        })?;
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let target = context.resolve_table_name(&requested);
        let table = target.as_ref().unwrap_or(&requested);
        let mut staged = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let skipped = ast.if_exists()
            && context
                .resolve_relation_name(requested.full_name())
                .is_none();
        let mut commands = Vec::new();
        let mut retired = Vec::new();
        // PostgreSQL executes column removals before column-attribute changes.
        let mut ordered: Vec<_> = ast.commands().iter().collect();
        ordered.sort_by_key(|command| {
            if Self::syntax(command).is_some() {
                0
            } else {
                ColumnAttributeAction::phase_from_ast_command(command).unwrap_or(3) + 1
            }
        });
        for command in ordered {
            if let Some((name, if_exists, cascade)) = Self::syntax(command) {
                let mut change = Self::resolve(
                    context,
                    table,
                    staged.as_ref(),
                    &ColumnName::from(name),
                    if_exists,
                    cascade,
                    skipped,
                )?;
                change.only = ast.only();
                change.relation_if_exists = ast.if_exists();
                change.finish(context, &retired)?;
                if let Some(staged) = &mut staged {
                    change.stage(staged, &mut retired);
                }
                commands.push(AlterObjSubCommand::drop_column(&change));
            } else if ColumnAttributeAction::phase_from_ast_command(command).is_some() {
                commands.push(AlterObjSubCommand::from_ast_column_attribute(
                    command,
                    context,
                    table,
                    &mut staged,
                    skipped,
                )?);
            } else {
                todo!("DROP COLUMN combined with other ALTER operations (#1077)");
            }
        }
        Ok(SemAst::alter_table(&AlterTable::analyzed_drops(
            &requested,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}

#[cfg(test)]
mod analysis_contract_tests;
