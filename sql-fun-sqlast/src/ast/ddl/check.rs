//! Thin CHECK adapters; shared expression validation belongs to CheckConstraint.
#[cfg(feature = "sqlite")]
use crate::ast::Expr;
use crate::{
    ast::{Column, ColumnConstraint, Table, TableConstraint, TryFromWithTokens},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CheckColumnConstraint, CheckConstraint,
        ColumnConstraint as SemColumnConstraint, ColumnDefinition, Constraint, CreateTable,
        ScalarAnalysisContext, problem::CheckProblem,
    },
    syn::{Constraint as SynConstraint, ScanToken},
};
use ::sql_fun_core::IVec;

impl TableConstraint {
    pub(crate) fn unnamed_check(constraint: &ColumnConstraint) -> Self {
        Self {
            extras: Default::default(),
            name: None,
            keys: Vec::new(),
            constraint: constraint.clone(),
            order: Vec::new(),
            auto_increment: false,
        }
    }

    pub(crate) fn resolve_legacy_inline_check<TContext: ParseContext>(
        &self,
        mut context: TContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<(CheckColumnConstraint, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let scope = if let Some(table) = &table {
            ScalarAnalysisContext::declared_table(table, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let valid = self.check_engine_features(&mut context, true)?;
        let result = self.resolve_check_expression(&mut context, &canonical, &scope, false);
        let mut check = context.finish_analysis(result)?;
        if !valid {
            check.disallow();
        }
        Ok((
            CheckColumnConstraint::from_analyzed(&canonical, column, &check),
            context,
        ))
    }

    pub(in crate::ast) const fn is_check(&self) -> bool {
        matches!(self.constraint, ColumnConstraint::Check(_))
    }

    pub(in crate::ast) fn resolve_check(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        alter: bool,
    ) -> Result<CheckConstraint, AnalysisError> {
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::default()
        };
        let mut check = self.resolve_check_expression(context, target, &scope, alter)?;
        let name = check.name().to_owned();
        if table.is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(target))?;
            check.missing_target();
        }
        if !name.is_empty() && table.is_some_and(|table| table.has_constraint_name(&name)) {
            check.reject_duplicate_name(context)?;
        }
        Ok(check)
    }

    pub(in crate::ast) fn resolve_check_expression(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
        alter: bool,
    ) -> Result<CheckConstraint, AnalysisError> {
        let ColumnConstraint::Check(syntax) = &self.constraint else {
            return AnalysisError::raise_unexpected_input("expected CHECK");
        };
        let expression = syntax.resolve_in(context, scope)?;
        let mut check = CheckConstraint::analyze(
            context,
            target,
            self.name.as_deref().unwrap_or_default(),
            &expression,
        )?;
        check.retain_options(
            self.extras.no_inherit,
            self.extras.skip_validation,
            self.extras.enforced,
        );
        if self.extras.deferrable || self.extras.initially_deferred {
            check.reject(context, CheckProblem::Deferrability)?;
        }
        check.set_validated(!alter || !self.extras.skip_validation);
        Ok(check)
    }

    pub(crate) fn analyze_legacy_check(
        context: &mut impl ParseContext,
        target: Option<&TableName>,
        syntax: &SynConstraint,
    ) -> Result<Constraint, AnalysisError> {
        let Some(target) = target else {
            return AnalysisError::raise_unexpected_none("CHECK target table");
        };
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let common = Self::try_from_with_tokens(syntax, &IVec::<ScanToken>::default())?;
        let valid = common.check_engine_features(context, false)?;
        let mut check = common.resolve_check(context, &canonical, table.as_ref(), true)?;
        if !valid {
            check.disallow();
        }
        Ok(Constraint::Check(check))
    }
}

impl Table {
    fn column_constraint_name_used(&self, context: &impl ParseContext, name: &str) -> bool {
        for column in &self.columns {
            for definition in &column.constraints {
                if !definition.is_check()
                    && definition.name.as_deref() == Some(name)
                    && definition.constraint.has_catalog_name(context)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn resolve_inline_checks(
        &self,
        context: &mut impl ParseContext,
        table: &mut CreateTable,
    ) -> Result<(), AnalysisError> {
        if !self.columns.iter().any(Column::has_inline_checks) {
            return Ok(());
        }
        let target =
            TableName::from(context.creation_name(table.name().full_name(), self.temporary)?);
        for syntax in &self.columns {
            if !syntax.has_inline_checks() {
                continue;
            }
            let name = ColumnName::from(syntax.name.as_str());
            let mut column = table
                .get_column_def(&name)
                .expect("declared column")
                .clone();
            syntax.resolve_inline_checks(context, &target, Some(table), false, &mut column)?;
            table.replace_declared_column(&column);
        }
        Ok(())
    }

    pub(super) fn resolve_checks(
        &self,
        context: &mut impl ParseContext,
        table: &mut CreateTable,
    ) -> Result<(), AnalysisError> {
        if !self.constraints.iter().any(TableConstraint::is_check) {
            return Ok(());
        }
        let target = context.creation_name(table.name().full_name(), self.temporary)?;
        for definition in &self.constraints {
            if definition.is_check() {
                let mut check = definition.resolve_check(
                    context,
                    &TableName::from(target.clone()),
                    Some(table),
                    false,
                )?;
                if !check.name().is_empty()
                    && self.column_constraint_name_used(context, check.name())
                {
                    check.reject_duplicate_name(context)?;
                }
                table.record_declared_constraint(&Constraint::Check(check))?;
            }
        }
        Ok(())
    }
}

impl Column {
    fn has_inline_checks(&self) -> bool {
        self.constraints.iter().any(TableConstraint::is_check)
    }

    pub(super) fn resolve_inline_checks(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        alter: bool,
        column: &mut ColumnDefinition,
    ) -> Result<(), AnalysisError> {
        if !self.has_inline_checks() {
            return Ok(());
        }
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::column_declaration(context, target, column)
        };
        let name = ColumnName::from(self.name.as_str());
        let mut index = 0;
        let mut names = Vec::new();
        for definition in &self.constraints {
            if definition.is_check() {
                let mut check =
                    definition.resolve_check_expression(context, target, &scope, alter)?;
                if !check.name().is_empty() {
                    if table.is_some_and(|table| table.has_constraint_name(check.name()))
                        || names.contains(&check.name().to_owned())
                    {
                        check.reject_duplicate_name(context)?;
                    }
                    names.push(check.name().to_owned());
                }
                let check = CheckColumnConstraint::from_analyzed(target, &name, &check);
                column.insert_inline_check(index, &SemColumnConstraint::check(check));
            }
            if !matches!(definition.constraint, ColumnConstraint::Enforcement(_)) {
                index += 1;
            }
        }
        Ok(())
    }
}

impl ColumnConstraint {
    #[cfg(feature = "sqlite")]
    pub(crate) fn check(expression: &Expr) -> Self {
        Self::Check(expression.clone())
    }

    pub(super) fn resolve_standalone_check(
        &self,
        context: &mut impl ParseContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<SemColumnConstraint, AnalysisError> {
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let definition = context.get_table(&canonical).cloned();
        let scope = if let Some(definition) = &definition {
            ScalarAnalysisContext::declared_table(definition, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let syntax = TableConstraint::unnamed_check(self);
        let check = syntax.resolve_check_expression(context, &canonical, &scope, false)?;
        Ok(SemColumnConstraint::check(
            CheckColumnConstraint::from_analyzed(&canonical, column, &check),
        ))
    }
}
