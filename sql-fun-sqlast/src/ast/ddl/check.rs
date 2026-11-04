//! Thin CHECK adapters; shared expression validation belongs to CheckConstraint.
use crate::{
    ast::{ColumnConstraint, Table, TableConstraint, TryFromWithTokens},
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, CheckConstraint, Constraint, CreateTable,
        ScalarAnalysisContext, problem::CheckProblem,
    },
    syn::{Constraint as SynConstraint, ScanToken},
};
use ::sql_fun_core::IVec;

impl TableConstraint {
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
        let ColumnConstraint::Check(syntax) = &self.constraint else {
            return AnalysisError::raise_unexpected_input("expected CHECK");
        };
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::default()
        };
        let expression = syntax.resolve_in(context, &scope)?;
        let name = self.name.as_deref().unwrap_or_default();
        let mut check = CheckConstraint::analyze(context, target, name, &expression)?;
        check.retain_options(
            self.extras.no_inherit,
            self.extras.skip_validation,
            self.extras.enforced,
        );
        if table.is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(target))?;
            check.missing_target();
        }
        if !name.is_empty() && table.is_some_and(|table| table.constraint(name).is_some()) {
            check.reject_duplicate_name(context)?;
        }
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
    pub(super) fn check_name_used(&self, name: &str) -> bool {
        self.constraints
            .iter()
            .any(|definition| definition.is_check() && definition.name.as_deref() == Some(name))
    }

    fn column_constraint_name_used(&self, context: &impl ParseContext, name: &str) -> bool {
        for column in &self.columns {
            for definition in &column.constraints {
                if definition.name.as_deref() == Some(name)
                    && definition.constraint.has_catalog_name(context)
                {
                    return true;
                }
            }
        }
        false
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
