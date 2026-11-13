//! Row-security target validation and independent table-state operations.

use crate::{
    ast::{AlterCommand, AlterRelation, Dialect, ddl::AlterOperation},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, Kind, SemAst,
        SourcePathWithSpan, search_path::expand_relation_search_path,
    },
};

/// An independent change to a `PostgreSQL` table's row-security flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowSecurityAction {
    /// Enable policy enforcement.
    Enable,
    /// Disable policy enforcement without clearing FORCE.
    Disable,
    /// Subject the owner to policies when enforcement is enabled.
    Force,
    /// Restore the owner's usual policy exemption without disabling enforcement.
    NoForce,
}

impl RowSecurityAction {
    /// Enable row-security enforcement.
    pub const fn enable() -> Self {
        Self::Enable
    }
    /// Disable enforcement, preserving the FORCE flag.
    pub const fn disable() -> Self {
        Self::Disable
    }
    /// Apply enabled enforcement to the table owner as well.
    pub const fn force() -> Self {
        Self::Force
    }
    /// Restore the usual owner exemption.
    pub const fn no_force() -> Self {
        Self::NoForce
    }

    pub(crate) const fn apply(self, enabled: &mut bool, forced: &mut bool) {
        match self {
            Self::Enable => *enabled = true,
            Self::Disable => *enabled = false,
            Self::Force => *forced = true,
            Self::NoForce => *forced = false,
        }
    }

    fn from_ast(command: &AlterCommand) -> Option<Self> {
        let AlterCommand::Postgres(command) = command else {
            return None;
        };
        match command.operation() {
            AlterOperation::EnableRowSecurity => Some(Self::Enable),
            AlterOperation::DisableRowSecurity => Some(Self::Disable),
            AlterOperation::ForceRowSecurity => Some(Self::Force),
            AlterOperation::NoForceRowSecurity => Some(Self::NoForce),
            _ => None,
        }
    }
}

/// Completed target validation, independent of diagnostic delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowSecurityOutcome {
    /// The resolved table accepts the operation.
    Applicable,
    /// IF EXISTS skips a missing relation.
    Skipped,
    /// No relation matched the requested identity.
    MissingTarget,
    /// A relation exists, but its kind cannot have table row-security flags.
    InvalidTarget(Kind),
    /// The target is a table, but the ALTER statement requested another kind.
    InvalidStatementKind,
}

impl RowSecurityOutcome {
    /// The resolved table accepts the operation.
    pub const fn applicable() -> Self {
        Self::Applicable
    }
    /// IF EXISTS skipped the missing target.
    pub const fn skipped() -> Self {
        Self::Skipped
    }
    /// No relation matched the requested identity.
    pub const fn missing_target() -> Self {
        Self::MissingTarget
    }
    /// The resolved relation has an incompatible kind.
    pub const fn invalid_target(kind: Kind) -> Self {
        Self::InvalidTarget(kind)
    }

    /// A non-table ALTER statement cannot apply row-security actions to a table.
    pub const fn invalid_statement_kind() -> Self {
        Self::InvalidStatementKind
    }

    const fn can_apply(self) -> bool {
        matches!(self, Self::Applicable)
    }

    const fn requires_diagnostic(self) -> bool {
        matches!(self, Self::MissingTarget | Self::InvalidTarget(_))
    }
}

/// Analyzed operation; its enclosing ALTER retains the target identity.
#[derive(Clone, Debug)]
pub struct RowSecurityChange {
    action: RowSecurityAction,
    outcome: RowSecurityOutcome,
}

impl RowSecurityChange {
    const fn new(action: RowSecurityAction, outcome: RowSecurityOutcome) -> Self {
        Self { action, outcome }
    }

    /// Operation selected by the SQL statement.
    pub const fn action(&self) -> RowSecurityAction {
        self.action
    }

    /// Completed target lookup/validation outcome.
    pub const fn outcome(&self) -> RowSecurityOutcome {
        self.outcome
    }

    /// Whether the operation may change the catalog.
    pub const fn can_apply(&self) -> bool {
        self.outcome.can_apply()
    }

    pub(crate) fn is_ast_relation(ast: &AlterRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| RowSecurityAction::from_ast(command).is_some())
    }

    pub(crate) fn analyze_relation(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return AnalysisError::raise_unexpected_input("PostgreSQL row-security syntax");
        }
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER TABLE source");
        };
        let requested = FullName::from_ast_name(ast.name())?;
        let binding = expand_relation_search_path(context, &requested, |key| {
            context.object_kind(key).map(|kind| (key.clone(), kind))
        });
        let (table, target, mut outcome) = match binding {
            Some((name, kind)) if kind == Kind::table() => {
                let table = TableName::from(name);
                (table.clone(), Some(table), RowSecurityOutcome::applicable())
            }
            Some((name, kind)) => (
                TableName::from(name),
                None,
                RowSecurityOutcome::invalid_target(kind),
            ),
            None if ast.if_exists() => (
                TableName::from(requested),
                None,
                RowSecurityOutcome::skipped(),
            ),
            None => (
                TableName::from(requested),
                None,
                RowSecurityOutcome::missing_target(),
            ),
        };
        if target.is_some() && !ast.kind().supports_row_security() {
            outcome = RowSecurityOutcome::invalid_statement_kind();
            context.report_analysis_problem(AnalysisProblem::row_security_requires_table())?;
        } else if outcome.requires_diagnostic() {
            context.report_analysis_problem(AnalysisProblem::table_not_found(&table))?;
        }
        let mut commands = Vec::new();
        for command in ast.commands() {
            let Some(action) = RowSecurityAction::from_ast(command) else {
                todo!("row security combined with other ALTER operations (#1104)");
            };
            commands.push(AlterObjSubCommand::row_security(&Self::new(
                action, outcome,
            )));
        }
        Ok(SemAst::alter_table(&AlterTable::row_security(
            &table,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}
