//! PostgreSQL constraint-change analysis.

use sql_fun_core::{PostgresVersion, analysis_problem::AlterConstraintReason};

use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation as AstAlterRelation, Deferrability,
        ddl::{AlterDefinition, AlterOperation, PostgresAlterCommand},
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem, ColumnConstraint,
        ColumnConstraintTrait, ColumnDefinition, Constraint, CreateTable, SemAst,
        SourcePathWithSpan, traversal::SemanticVisitor,
    },
};

/// An ALTER TABLE constraint operation and its completed local analysis.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterConstraint {
    name: String,
    table: Option<TableName>,
    action: ConstraintAction,
    target: ConstraintTarget,
    columns: Vec<ColumnDefinition>,
    owned_index: Option<FullName>,
    #[getter(copy)]
    only: bool,
    #[getter(copy)]
    relation_if_exists: bool,
    #[getter(copy)]
    outcome: ConstraintChangeOutcome,
}

/// Requested behavior for the named constraint.
#[derive(Debug, Clone)]
pub enum ConstraintAction {
    /// Set deferrability, or retain it when PostgreSQL 18 receives no attributes.
    Alter(Option<Deferrability>),
    /// Validate the existing constraint.
    Validate,
    /// Remove the constraint according to the requested DROP options.
    Drop {
        /// Permit a missing constraint.
        if_exists: bool,
        /// Request removal of dependent objects.
        cascade: bool,
    },
}

/// Completed lookup of the constraint declaration.
#[derive(Debug, Clone)]
pub enum ConstraintTarget {
    /// Constraint retained in the table constraint collection.
    Table(Box<Constraint>),
    /// Constraint declared inside a column definition.
    Inline(Box<ColumnConstraint>),
    /// The relation lookup failed or was skipped by ALTER TABLE IF EXISTS.
    NoTable,
    /// The target table has no matching retained constraint.
    Missing,
    /// More than one retained declaration matches.
    Ambiguous,
}

/// Local outcome available before schema-catalog application.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConstraintChangeOutcome {
    /// The operation passed local validation.
    Valid,
    /// Conditional miss or an operation that would not change the constraint.
    NoOp,
    /// The operation failed local validation.
    Rejected,
}

fn is_postgres18_or_later(context: &impl ParseContext) -> bool {
    context
        .metadata()
        .engine_version()
        .as_ref()
        .is_some_and(|version| {
            version
                .to_string()
                .parse::<PostgresVersion>()
                .is_ok_and(|version| version.major() >= 18)
        })
}

impl ConstraintAction {
    fn from_ast<'ast>(
        command: &'ast PostgresAlterCommand,
        context: &impl ParseContext,
    ) -> Result<(&'ast str, Self), AnalysisError> {
        let action = match command.operation() {
            AlterOperation::AlterConstraint => {
                let AlterDefinition::ConstraintFlags(flags) = command.definition() else {
                    return AnalysisError::raise_unexpected_input("ALTER CONSTRAINT flags");
                };
                if flags.enforced().is_some() || flags.no_inherit().is_some() {
                    todo!("ALTER CONSTRAINT enforcement/inheritance (#887)");
                }
                let deferrability = if let Some((deferrable, deferred)) = flags.deferrability() {
                    Some(Deferrability::new(deferrable, deferred))
                } else if is_postgres18_or_later(context) {
                    None
                } else {
                    Some(Deferrability::new(false, false))
                };
                (flags.name().as_str(), Self::Alter(deferrability))
            }
            AlterOperation::ValidateConstraint => (command.name().as_str(), Self::Validate),
            AlterOperation::DropConstraint => (
                command.name().as_str(),
                Self::Drop {
                    if_exists: command.missing_ok(),
                    cascade: command.cascade(),
                },
            ),
            _ => return AnalysisError::raise_unexpected_input("constraint change"),
        };
        Ok(action)
    }
}

impl ConstraintTarget {
    fn lookup(table: &CreateTable, name: &str, context: &impl ParseContext) -> Self {
        let postgres18 = is_postgres18_or_later(context);
        let table_constraint = table
            .constraint(name)
            .filter(|constraint| postgres18 || !matches!(constraint, Constraint::NotNull(..)));
        let mut inline = table
            .named_inline_constraints(name)
            .filter(|constraint| postgres18 || constraint.as_nullability().is_none());
        match (table_constraint, inline.next(), inline.next()) {
            (Some(constraint), None, _) => Self::Table(Box::new(constraint.clone())),
            (None, Some(constraint), None) => Self::Inline(Box::new(constraint.clone())),
            (None, None, _) => Self::Missing,
            _ => Self::Ambiguous,
        }
    }

    fn validation_state(&self) -> Option<(bool, bool)> {
        match self {
            Self::Table(constraint) => match constraint.as_ref() {
                Constraint::Check(check) => {
                    Some((check.enforced() != Some(false), check.validated()))
                }
                Constraint::NotNull(value, _) => Some((true, value.is_validated())),
                Constraint::Foreign(key) => Some((
                    true,
                    key.validity().is_none_or(|value| !value.skips_validation()),
                )),
                _ => None,
            },
            Self::Inline(constraint) => {
                if let Some(check) = constraint.as_check() {
                    let enforced = check.enforced() != Some(false);
                    Some((enforced, enforced))
                } else if let Some(key) = constraint.as_foreign_key() {
                    Some((true, key.is_validated()))
                } else {
                    constraint
                        .as_nullability()
                        .map(|value| (true, value.is_validated()))
                }
            }
            _ => None,
        }
    }

    fn foreign_key_deferrability(&self) -> Option<Option<&Deferrability>> {
        match self {
            Self::Table(constraint) => constraint.as_foreign_key().map(|key| key.deferrability()),
            Self::Inline(constraint) => constraint.as_foreign_key().map(|key| key.deferrability()),
            _ => None,
        }
    }

    fn column_names(&self) -> Vec<ColumnName> {
        match self {
            Self::Inline(value) => vec![value.column_name().clone()],
            Self::Table(constraint) => {
                if let Constraint::NotNull(value, _) = constraint.as_ref() {
                    vec![value.column_name().clone()]
                } else if let Some(key) = constraint.as_foreign_key() {
                    key.source_columns().to_vec()
                } else {
                    constraint.unique_key_columns()
                }
            }
            _ => Vec::new(),
        }
    }

    fn owned_index(
        &self,
        table: &TableName,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        if let Self::Table(constraint) = self
            && matches!(
                constraint.as_ref(),
                Constraint::PrimaryKey(_) | Constraint::Unique(_)
            )
            && let Some(change) = constraint.implicit_changes()?.into_iter().next()
        {
            let index = change.into_index().with_table_identity(table.full_name());
            return Ok(context
                .get_index_impl(index.name())
                .map(|index| index.name().clone()));
        }
        Ok(None)
    }
}

impl AlterConstraint {
    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Some(table) = &self.table {
            for column in &self.columns {
                if let Some(name) = column.name() {
                    visitor.schema_column(table.full_name(), false, name)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn is_ast_relation(ast: &AstAlterRelation) -> bool {
        ast.commands()
            .iter()
            .any(|command| Self::ast_command(command).is_some())
    }

    pub(crate) fn from_ast_command(
        command: &PostgresAlterCommand,
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Option<AlterObjSubCommand>, AnalysisError> {
        if !Self::is_operation(command.operation()) {
            return Ok(None);
        }
        let table = context.resolve_table_name(requested);
        let definition = if let Some(table) = &table {
            context.get_table_impl(table.full_name()).cloned()
        } else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            None
        };
        let change = Self::resolve(command, context, table.as_ref(), definition.as_ref(), false)?;
        Ok(Some(AlterObjSubCommand::ChangeConstraint(Box::new(change))))
    }

    const fn is_operation(operation: AlterOperation) -> bool {
        matches!(
            operation,
            AlterOperation::AlterConstraint
                | AlterOperation::ValidateConstraint
                | AlterOperation::DropConstraint
        )
    }

    fn ast_command(ast: &AstAlterCommand) -> Option<&PostgresAlterCommand> {
        if let AstAlterCommand::Postgres(command) = ast
            && Self::is_operation(command.operation())
        {
            Some(command)
        } else {
            None
        }
    }

    pub(crate) fn analyze_relation(
        ast: &AstAlterRelation,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER constraint source span");
        };
        let requested = TableName::from(FullName::from_ast_name(ast.name())?);
        let table = context.resolve_table_name(&requested);
        let definition = if let Some(table) = &table {
            context.get_table_impl(table.full_name()).cloned()
        } else {
            None
        };
        let skipped = table.is_none()
            && ast.if_exists()
            && context
                .resolve_relation_name(requested.full_name())
                .is_none();
        if table.is_none() && !skipped {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
        }
        let mut commands = Vec::new();
        for command in ast.commands() {
            if let Some(command) = Self::ast_command(command) {
                let mut change = Self::resolve(
                    command,
                    context,
                    table.as_ref(),
                    definition.as_ref(),
                    skipped,
                )?;
                change.only = ast.only();
                change.relation_if_exists = ast.if_exists();
                commands.push(AlterObjSubCommand::ChangeConstraint(Box::new(change)));
            } else {
                commands.push(AlterObjSubCommand::from_ast(
                    command,
                    context,
                    table.as_ref().unwrap_or(&requested),
                )?);
            }
        }
        Ok(SemAst::alter_table(&AlterTable::resolved_commands(
            &requested,
            table.as_ref(),
            &commands,
            source,
        )))
    }

    fn resolve(
        command: &PostgresAlterCommand,
        context: &mut impl ParseContext,
        table: Option<&TableName>,
        definition: Option<&CreateTable>,
        skipped: bool,
    ) -> Result<Self, AnalysisError> {
        let (name, action) = ConstraintAction::from_ast(command, context)?;
        let target = if let Some(definition) = definition {
            ConstraintTarget::lookup(definition, name, context)
        } else {
            ConstraintTarget::NoTable
        };
        let outcome = Self::validate(context, name, &target, &action, skipped)?;
        let columns = if let Some(definition) = definition {
            target
                .column_names()
                .iter()
                .filter_map(|name| definition.get_column_def(name).cloned())
                .collect()
        } else {
            Vec::new()
        };
        let owned_index = if let Some(table) = table {
            target.owned_index(table, context)?
        } else {
            None
        };
        Ok(Self {
            name: name.to_owned(),
            table: table.cloned(),
            action,
            target,
            columns,
            owned_index,
            only: false,
            relation_if_exists: false,
            outcome,
        })
    }

    fn validate(
        context: &mut impl ParseContext,
        name: &str,
        target: &ConstraintTarget,
        action: &ConstraintAction,
        skipped: bool,
    ) -> Result<ConstraintChangeOutcome, AnalysisError> {
        use ConstraintChangeOutcome::{NoOp, Rejected, Valid};
        match target {
            ConstraintTarget::NoTable => return Ok(if skipped { NoOp } else { Rejected }),
            ConstraintTarget::Missing
                if matches!(
                    action,
                    ConstraintAction::Drop {
                        if_exists: true,
                        ..
                    }
                ) =>
            {
                return Ok(NoOp);
            }
            ConstraintTarget::Missing => {
                context.report_analysis_problem(AnalysisProblem::AlterConstraintNotFound(
                    name.to_owned(),
                ))?;
                return Ok(Rejected);
            }
            ConstraintTarget::Ambiguous => {
                context.report_analysis_problem(AnalysisProblem::AlterConstraintAmbiguous(
                    name.to_owned(),
                ))?;
                return Ok(Rejected);
            }
            _ => {}
        }
        let reason = match action {
            ConstraintAction::Drop { .. } => return Ok(Valid),
            ConstraintAction::Alter(None) => return Ok(NoOp),
            ConstraintAction::Alter(Some(deferrability)) => {
                if let Some(current) = target.foreign_key_deferrability() {
                    let current = current
                        .cloned()
                        .unwrap_or_else(|| Deferrability::new(false, false));
                    return Ok(if &current == deferrability {
                        NoOp
                    } else {
                        Valid
                    });
                }
                AlterConstraintReason::Deferrability
            }
            ConstraintAction::Validate => match target.validation_state() {
                Some((false, _)) => AlterConstraintReason::NotEnforced,
                Some((true, true)) => return Ok(NoOp),
                Some((true, false)) => return Ok(Valid),
                None => AlterConstraintReason::ValidationKind,
            },
        };
        context.report_analysis_problem(AnalysisProblem::AlterConstraintInvalid(
            name.to_owned(),
            reason,
        ))?;
        Ok(Rejected)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::Dialect,
        context::TableInfoRead,
        names::FullName,
        parse_statements,
        sem::{
            AlterObjSubCommand, AlterTable, ConstraintAction, ConstraintChangeOutcome,
            ConstraintTarget,
        },
        test_helpers::foreign_keys as fk,
    };

    #[test]
    fn inline_fk_retains_deferrability_and_drop_retains_index_without_applying()
    -> testresult::TestResult {
        let context = fk::apply(
            fk::parents(),
            "CREATE TABLE a.child (id integer CONSTRAINT child_fk REFERENCES a.parent(id) DEFERRABLE INITIALLY DEFERRED)",
        );
        let statement = parse_statements(
            "ALTER TABLE a.child ALTER CONSTRAINT child_fk",
            Dialect::postgres(),
        )
        .next()
        .unwrap()?;
        let analyzed = AlterTable::analyze(&statement, context)?;
        let alter = analyzed.ast().alter_table_statement().unwrap();
        let AlterObjSubCommand::ChangeConstraint(change) = &alter.commands()[0] else {
            panic!("expected constraint change");
        };
        assert_eq!(change.table().as_ref(), Some(&fk::name("a", "child")));
        assert!(matches!(change.target(), ConstraintTarget::Inline(_)));
        let ConstraintAction::Alter(Some(deferrability)) = change.action() else {
            panic!("expected effective deferrability");
        };
        assert!(!deferrability.is_deferrable());
        assert!(!deferrability.is_initially_deferred());
        assert_eq!(change.outcome(), ConstraintChangeOutcome::Valid);
        assert_eq!(change.columns().len(), 1);

        let context = analyzed.into_context();
        let statement = parse_statements(
            "ALTER TABLE a.parent DROP CONSTRAINT parent_pk CASCADE",
            Dialect::postgres(),
        )
        .next()
        .unwrap()?;
        let analyzed = AlterTable::analyze(&statement, context)?;
        let alter = analyzed.ast().alter_table_statement().unwrap();
        assert!(alter.requires_application_validation());
        let AlterObjSubCommand::ChangeConstraint(change) = &alter.commands()[0] else {
            panic!("expected constraint change");
        };
        assert_eq!(
            change.owned_index().as_ref(),
            Some(&FullName::with_schema("a", "parent_pk"))
        );
        assert!(matches!(
            change.action(),
            ConstraintAction::Drop { cascade: true, .. }
        ));
        let context = fk::apply(
            analyzed.into_context(),
            "ALTER TABLE a.parent DROP CONSTRAINT parent_pk CASCADE",
        );
        let parent = context
            .get_table_impl(&FullName::with_schema("a", "parent"))
            .unwrap();
        assert!(parent.constraint("parent_pk").is_some());
        assert!(parent.alter_statements().is_empty());
        Ok(())
    }
}
