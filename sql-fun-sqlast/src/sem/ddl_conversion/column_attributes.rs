use crate::ast::ddl::{AlterOperation, PostgresAlterCommand};
use crate::ast::{AlterCommand, AlterRelation, Expr, Literal};
use crate::sem::{AstConversion, convert};

use crate::{
    ast::{SyntaxOption, SyntaxValue, ddl::AlterDefinition},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AlterColumnAttributes, AlterObjSubCommand, AlterTable, AnalysisError, AnalysisProblem,
        ColumnAttributeAction, ColumnAttributeBinding, ColumnAttributeOption, CreateTable, Kind,
        SemAst, SourcePathWithSpan, problem::AlterColumnProblem,
        search_path::expand_relation_search_path,
    },
};

impl AstConversion<'_, AlterRelation> {
    pub(crate) fn has_column_attributes(&self) -> bool {
        self.ast
            .commands
            .iter()
            .any(|command| convert(command).column_attribute_phase().is_some())
    }
    pub(crate) fn resolve_column_attributes(
        &self,
        context: &mut impl ParseContext,
        source: Option<&SourcePathWithSpan>,
    ) -> Result<SemAst, AnalysisError> {
        // Other relation kinds retain their own dispatch boundary.
        if !self.ast.kind.supports_table_column_attributes() {
            todo!("column attributes on non-table relations");
        }
        if !self
            .commands
            .iter()
            .all(|command| convert(command).column_attribute_phase().is_some())
        {
            todo!("column attributes mixed with other ALTER operations");
        }
        let Some(source) = source else {
            return AnalysisError::raise_unexpected_none("ALTER COLUMN source");
        };
        let requested = TableName::from(convert(&self.ast.name).full_name()?);
        let target = context.resolve_table_name(&requested);
        let mut staged = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let table = target.as_ref().unwrap_or(&requested);
        let mut ordered: Vec<_> = self.ast.commands.iter().collect();
        // PostgreSQL AT_PASS_DROP, AT_PASS_COL_ATTRS, AT_PASS_MISC. Stable sort
        // preserves the supplied order within a pass, including repeated changes.
        ordered.sort_by_key(|command| convert(*command).column_attribute_phase());
        let existing_relation =
            expand_relation_search_path(context, requested.full_name(), |name| {
                context.object_kind(name)
            });
        if existing_relation == Some(Kind::materialized_view())
            || existing_relation == Some(Kind::index())
        {
            todo!("column attributes on materialized views and indexes");
        }
        let skip_missing = self.ast.if_exists && existing_relation.is_none() && target.is_none();
        let mut commands = Vec::new();
        for command in ordered {
            commands.push(convert(command).resolve_column_attribute(
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
    use crate::sem::convert;
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
        let statement =
            convert(&relation).resolve_column_attributes(&mut context, Some(&source_span()))?;
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

impl AstConversion<'_, AlterCommand> {
    pub(crate) fn column_attribute_phase(&self) -> Option<u8> {
        if let AlterCommand::Postgres(command) = self.ast {
            convert(&command.operation).column_attribute_phase()
        } else {
            None
        }
    }
    pub(crate) fn resolve_column_attribute(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let AlterCommand::Postgres(command) = self.ast else {
            unreachable!("checked attribute command");
        };
        convert::<PostgresAlterCommand>(command)
            .resolve_column_attribute(context, table, staged, if_exists)
    }
}

impl AstConversion<'_, AlterOperation> {
    pub(crate) const fn drops_not_null(self) -> bool {
        matches!(self.ast, AlterOperation::DropNotNull)
    }
    pub(crate) const fn drops_expression(self) -> bool {
        matches!(self.ast, AlterOperation::DropExpression)
    }
    pub(crate) const fn sets_statistics(self) -> bool {
        matches!(self.ast, AlterOperation::SetStatistics)
    }
    pub(crate) fn column_attribute_action(
        self,
        definition: &AlterDefinition,
        missing_ok: bool,
    ) -> Result<ColumnAttributeAction, AnalysisError> {
        match self.ast {
            AlterOperation::SetNotNull => Ok(ColumnAttributeAction::not_null(true)),
            AlterOperation::DropNotNull => Ok(ColumnAttributeAction::not_null(false)),
            AlterOperation::DropExpression => {
                Ok(ColumnAttributeAction::drop_expression(missing_ok))
            }
            AlterOperation::SetStatistics => Ok(ColumnAttributeAction::statistics(
                convert(definition).statistics_request()?,
            )),
            AlterOperation::SetOptions => Ok(ColumnAttributeAction::options(
                &convert(definition).column_options()?,
                false,
            )),
            AlterOperation::ResetOptions => Ok(ColumnAttributeAction::options(
                &convert(definition).column_options()?,
                true,
            )),
            _ => unreachable!("checked attribute operation"),
        }
    }
    pub(crate) const fn column_attribute_phase(self) -> Option<u8> {
        match self.ast {
            AlterOperation::DropNotNull | AlterOperation::DropExpression => Some(0),
            AlterOperation::SetNotNull => Some(1),
            AlterOperation::SetStatistics
            | AlterOperation::SetOptions
            | AlterOperation::ResetOptions => Some(2),
            _ => None,
        }
    }
}

impl AstConversion<'_, PostgresAlterCommand> {
    pub(crate) fn resolve_single_column_attribute(
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
    pub(crate) fn resolve_column_attribute(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        staged: &mut Option<CreateTable>,
        if_exists: bool,
    ) -> Result<AlterObjSubCommand, AnalysisError> {
        let column = ColumnName::from(self.ast.name.as_str());
        let action = convert(&self.ast.operation)
            .column_attribute_action(&self.ast.definition, self.ast.missing_ok)?;
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
            if convert(&self.ast.operation).drops_not_null() {
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
            if convert(&self.ast.operation).drops_expression() {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                } else if !definition.has_stored_generation() && !self.ast.missing_ok {
                    problems.push(AlterColumnProblem::not_stored_generated());
                }
            }
            if convert(&self.ast.operation).sets_statistics() {
                if definition.has_virtual_generation() {
                    problems.push(AlterColumnProblem::virtual_generated());
                }
                convert(&self.ast.definition).validate_statistics(context, &mut problems)?;
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

impl AstConversion<'_, AlterDefinition> {
    pub(crate) fn statistics_request(&self) -> Result<Option<i32>, AnalysisError> {
        match self.ast {
            AlterDefinition::None => Ok(None),
            AlterDefinition::Expression(expr) => convert(expr)
                .statistics_integer()
                .map(Some)
                .ok_or_else(|| AnalysisError::unexpected_input_error("statistics integer")),
            _ => AnalysisError::raise_unexpected_input("statistics definition"),
        }
    }
    pub(crate) fn column_options(&self) -> Result<Vec<ColumnAttributeOption>, AnalysisError> {
        if let AlterDefinition::Options(options) = self.ast {
            options
                .iter()
                .map(|syntax| convert::<SyntaxOption>(syntax).column_attribute_option())
                .collect()
        } else {
            AnalysisError::raise_unexpected_input("column attribute options")
        }
    }
    pub(crate) fn validate_statistics(
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

impl AstConversion<'_, Expr> {
    pub(crate) const fn statistics_integer(&self) -> Option<i32> {
        if let Expr::Literal(value) = self.ast {
            convert(value).statistics_integer()
        } else {
            None
        }
    }
    pub(crate) fn column_option_text(&self) -> Result<String, AnalysisError> {
        if let Expr::Literal(value) = self.ast {
            convert(value).column_option_text()
        } else {
            AnalysisError::raise_unexpected_input("column option constant")
        }
    }
}

impl AstConversion<'_, Literal> {
    pub(crate) const fn statistics_integer(&self) -> Option<i32> {
        if let Literal::Integer(value) = self.ast {
            Some(*value)
        } else {
            None
        }
    }
    pub(crate) fn column_option_text(&self) -> Result<String, AnalysisError> {
        match self.ast {
            Literal::Integer(value) => Ok(value.to_string()),
            Literal::String(value) | Literal::Numeric(value) | Literal::Keyword(value) => {
                Ok(value.clone())
            }
            Literal::Boolean(value) => Ok(value.to_string()),
            _ => AnalysisError::raise_unexpected_input("column option literal"),
        }
    }
}

impl AstConversion<'_, SyntaxOption> {
    pub(crate) fn column_attribute_option(&self) -> Result<ColumnAttributeOption, AnalysisError> {
        let name = if let Some(namespace) = &self.ast.namespace {
            format!("{namespace}.{}", self.ast.name)
        } else {
            self.ast.name.clone()
        };
        let value = self
            .value
            .as_ref()
            .map(|syntax| convert::<SyntaxValue>(syntax).column_option_text())
            .transpose()?;
        Ok(ColumnAttributeOption::new(&name, value.as_deref()))
    }
}

impl AstConversion<'_, SyntaxValue> {
    pub(crate) fn column_option_text(&self) -> Result<String, AnalysisError> {
        match self.ast {
            SyntaxValue::Expression(value) => convert(value).column_option_text(),
            SyntaxValue::Type(value) => Ok(convert(&value.name).full_name()?.to_string()),
        }
    }
}
