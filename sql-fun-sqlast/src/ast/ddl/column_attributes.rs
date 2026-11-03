//! ALTER COLUMN attribute resolution. Unimplemented dependent operations remain leaves.

use crate::{
    ast::{
        AlterCommand, AlterRelation, AlterRelationKind, Expr, Literal, SyntaxOption, SyntaxValue,
        ddl::{AlterDefinition, AlterOperation},
    },
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AlterColumnAttributes, AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem,
        ColumnAttributeAction, ColumnAttributeBinding, ColumnAttributeOption, CreateTable, Kind,
        SemAst, SourcePathWithSpan, problem::AlterColumnProblem,
        search_path::expand_relation_search_path,
    },
};

impl AlterRelation {
    pub(super) fn has_column_attributes(&self) -> bool {
        self.commands
            .iter()
            .any(|command| command.column_attribute_phase().is_some())
    }
    pub(super) fn resolve_column_attributes(
        &self,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        // Other relation kinds retain their own dispatch boundary.
        if !self.kind.supports_table_column_attributes() {
            todo!("column attributes on non-table relations");
        }
        if !self
            .commands
            .iter()
            .all(|command| command.column_attribute_phase().is_some())
        {
            todo!("column attributes mixed with other ALTER operations");
        }
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER COLUMN source");
        };
        let requested = TableName::from(self.name.full_name()?);
        let target = context.resolve_table_name(&requested);
        let mut staged = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let table = target.as_ref().unwrap_or(&requested);
        let mut ordered: Vec<_> = self.commands.iter().collect();
        // PostgreSQL AT_PASS_DROP, AT_PASS_COL_ATTRS, AT_PASS_MISC. Stable sort
        // preserves the supplied order within a pass, including repeated changes.
        ordered.sort_by_key(|command| command.column_attribute_phase());
        let existing_relation =
            expand_relation_search_path(context, requested.full_name(), |name| {
                context.object_kind(name)
            });
        if existing_relation == Some(Kind::materialized_view())
            || existing_relation == Some(Kind::index())
        {
            todo!("column attributes on materialized views and indexes");
        }
        let skip_missing = self.if_exists && existing_relation.is_none() && target.is_none();
        let mut commands = Vec::new();
        for command in ordered {
            commands.push(command.resolve_column_attribute(
                context,
                table,
                &mut staged,
                skip_missing,
            )?);
        }
        Ok(SemAst::alter_table(&AlterTable::column_attributes(
            table,
            target.as_ref(),
            &commands,
            source,
        )))
    }
}

#[cfg(test)]
mod test_applicability {
    use crate::{
        ast::StatementNode,
        names::{ColumnName, FullName, TableName},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
            source_span,
        },
    };

    use crate::{ParseContext, context::BaseParseContext};

    #[test]
    fn discarded_diagnostics_cannot_make_rejected_ddl_applicable() -> testresult::TestResult {
        let context = ownership_context();
        let context = apply_ownership_sql(context, "CREATE TABLE t(a integer)")?.into_context();
        let syntax =
            ownership_statement("ALTER TABLE t ALTER a SET NOT NULL, ALTER a SET STATISTICS -2")?;
        let StatementNode::AlterRelation(relation) = syntax else {
            panic!("ALTER TABLE syntax");
        };
        let mut context = context;
        context.begin_parameter_type_analysis();
        let statement = relation.resolve_column_attributes(&mut context, Some(&source_span()))?;
        // Simulate a diagnostic policy that suppresses delivery after analysis.
        let problems = context.analysis_state_mut().take_problems();
        assert_eq!(problems.len(), 1);
        let problem = problems[0]
            .alter_column_problem()
            .expect("structured diagnostic");
        assert_eq!(problem.column().as_str(), "a");
        assert_eq!(problem.table().as_str(), "public.t");
        assert!(problem.reason().is_error());
        assert!(context.problems().is_empty());
        assert!(!statement.alter_table_statement().unwrap().can_apply());
        let context = statement.apply_relation_alter(context)?;
        let table = context
            .get_table(&TableName::from(FullName::with_schema("public", "t")))
            .unwrap();
        let column = table.get_column_def(&ColumnName::from("a")).unwrap();
        assert_eq!(column.is_not_null(), Some(false));
        assert_eq!(column.statistics_target(), None);
        let result = apply_ownership_sql(context, "SELECT a FROM t")?;
        assert!(result.ast().select_statement().is_some());
        Ok(())
    }
}

impl AlterCommand {
    fn column_attribute_phase(&self) -> Option<u8> {
        if let Self::Postgres(command) = self {
            command.operation.column_attribute_phase()
        } else {
            None
        }
    }
    fn resolve_column_attribute(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let Self::Postgres(command) = self else {
            unreachable!("checked attribute command");
        };
        command.resolve_column_attribute(context, table, staged, if_exists)
    }
}
impl AlterRelationKind {
    pub(super) const fn supports_table_column_attributes(self) -> bool {
        matches!(self, Self::Table)
    }
}
impl AlterOperation {
    const fn drops_not_null(self) -> bool {
        matches!(self, Self::DropNotNull)
    }
    const fn drops_expression(self) -> bool {
        matches!(self, Self::DropExpression)
    }
    const fn sets_statistics(self) -> bool {
        matches!(self, Self::SetStatistics)
    }
    fn column_attribute_action(
        self,
        definition: &AlterDefinition,
        missing_ok: bool,
    ) -> Result<ColumnAttributeAction, AnalysisError> {
        match self {
            Self::SetNotNull => Ok(ColumnAttributeAction::not_null(true)),
            Self::DropNotNull => Ok(ColumnAttributeAction::not_null(false)),
            Self::DropExpression => Ok(ColumnAttributeAction::drop_expression(missing_ok)),
            Self::SetStatistics => Ok(ColumnAttributeAction::statistics(
                definition.statistics_request()?,
            )),
            Self::SetOptions => Ok(ColumnAttributeAction::options(
                &definition.column_options()?,
                false,
            )),
            Self::ResetOptions => Ok(ColumnAttributeAction::options(
                &definition.column_options()?,
                true,
            )),
            _ => unreachable!("checked attribute operation"),
        }
    }
    pub(super) const fn column_attribute_phase(self) -> Option<u8> {
        match self {
            Self::DropNotNull | Self::DropExpression => Some(0),
            Self::SetNotNull => Some(1),
            Self::SetStatistics | Self::SetOptions | Self::ResetOptions => Some(2),
            _ => None,
        }
    }
}
impl super::PostgresAlterCommand {
    pub(super) fn resolve_single_column_attribute(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let resolved = context.resolve_table_name(table);
        let table = resolved.as_ref().unwrap_or(table);
        let mut staged = if let Some(name) = &resolved {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        self.resolve_column_attribute(context, table, &mut staged, false)
    }
    fn resolve_column_attribute(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let column = ColumnName::from(self.name.as_str());
        let action = self
            .operation
            .column_attribute_action(&self.definition, self.missing_ok)?;
        let mut problems = Vec::new();
        let binding = match staged.as_ref() {
            None => {
                if !if_exists {
                    problems.push(AlterColumnProblem::missing_relation());
                }
                ColumnAttributeBinding::missing_relation()
            }
            Some(definition) => match definition.get_column_def(&column) {
                None => {
                    problems.push(AlterColumnProblem::missing_column());
                    ColumnAttributeBinding::missing_column()
                }
                Some(definition) => ColumnAttributeBinding::resolved(definition),
            },
        };
        if let Some(definition) = binding.definition() {
            if self.operation.drops_not_null() {
                if definition.has_inline_primary_key()
                    || staged
                        .as_ref()
                        .is_some_and(|table| table.primary_key_columns().contains(&&column))
                {
                    problems.push(AlterColumnProblem::primary_key());
                }
                if definition.has_identity() {
                    problems.push(AlterColumnProblem::identity());
                }
            }
            if self.operation.drops_expression() {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                } else if !definition.has_stored_generation() && !self.missing_ok {
                    problems.push(AlterColumnProblem::not_stored_generated());
                }
            }
            if self.operation.sets_statistics() {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                }
                self.definition
                    .validate_statistics(context, &mut problems)?;
            }
            problems.extend(action.validate_options());
        }
        for problem in &problems {
            context
                .report_analysis_problem(AnalysisProblem::alter_column(table, &column, problem))?;
        }
        let change = AlterColumnAttributes::new(&column, &binding, &action, &problems);
        if let Some(staged) = staged {
            change.apply(staged);
        }
        Ok(AlterObjSubCommand::column_attributes(&change))
    }
}
impl AlterDefinition {
    fn statistics_request(&self) -> Result<Option<i32>, AnalysisError> {
        match self {
            Self::None => Ok(None),
            Self::Expression(expr) => expr
                .statistics_integer()
                .map(Some)
                .ok_or_else(|| AnalysisError::unexpected_input_error("statistics integer")),
            _ => AnalysisError::raise_unexpected_input("statistics definition"),
        }
    }
    fn column_options(&self) -> Result<Vec<ColumnAttributeOption>, AnalysisError> {
        if let Self::Options(options) = self {
            options
                .iter()
                .map(SyntaxOption::column_attribute_option)
                .collect()
        } else {
            AnalysisError::raise_unexpected_input("column attribute options")
        }
    }
    fn validate_statistics(
        &self,
        context: &impl ParseContext,
        problems: &mut Vec<AlterColumnProblem>,
    ) -> Result<(), AnalysisError> {
        match self.statistics_request()? {
            Some(value) if value < -1 => {
                problems.push(AlterColumnProblem::statistics_too_low(value))
            }
            Some(value) if value > 10000 => {
                problems.push(AlterColumnProblem::statistics_reduced(value))
            }
            None => {
                let configured = context.get_args().postgres_version(context.metadata())?;
                let Ok(version) = configured
                    .to_string()
                    .parse::<::sql_fun_core::PostgresVersion>()
                else {
                    todo!("custom PostgreSQL version capabilities");
                };
                if version.major() < 17 {
                    problems.push(AlterColumnProblem::version());
                }
            }
            _ => (),
        }
        Ok(())
    }
}
impl Expr {
    const fn statistics_integer(&self) -> Option<i32> {
        if let Self::Literal(value) = self {
            value.statistics_integer()
        } else {
            None
        }
    }
    fn column_option_text(&self) -> Result<String, AnalysisError> {
        if let Self::Literal(value) = self {
            value.column_option_text()
        } else {
            AnalysisError::raise_unexpected_input("column option constant")
        }
    }
}
impl Literal {
    const fn statistics_integer(&self) -> Option<i32> {
        if let Self::Integer(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    fn column_option_text(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Integer(value) => Ok(value.to_string()),
            Self::String(value) | Self::Numeric(value) | Self::Keyword(value) => Ok(value.clone()),
            Self::Boolean(value) => Ok(value.to_string()),
            _ => AnalysisError::raise_unexpected_input("column option literal"),
        }
    }
}
impl SyntaxOption {
    fn column_attribute_option(&self) -> Result<ColumnAttributeOption, AnalysisError> {
        let name = if let Some(namespace) = &self.namespace {
            format!("{namespace}.{}", self.name)
        } else {
            self.name.clone()
        };
        let value = self
            .value
            .as_ref()
            .map(SyntaxValue::column_option_text)
            .transpose()?;
        Ok(ColumnAttributeOption::new(&name, value.as_deref()))
    }
}
impl SyntaxValue {
    fn column_option_text(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Expression(value) => value.column_option_text(),
            Self::Type(value) => Ok(value.name.full_name()?.to_string()),
        }
    }
}
