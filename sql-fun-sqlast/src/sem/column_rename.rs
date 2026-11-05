//! Completed column-rename analysis and exact dependency matching.

mod ddl;
mod sqlite;

use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{AnalysisError, AnalysisProblem, ColumnDefinition},
};

/// A column rename retains its source binding and applicability independently of diagnostics.
#[derive(Debug, Clone)]
pub struct ColumnRename {
    table: TableName,
    requested: ColumnName,
    source: Option<ColumnDefinition>,
    destination: ColumnName,
    applicable: bool,
    rewrites_outputs: bool,
    sqlite_outputs: std::collections::HashMap<FullName, Vec<ColumnName>>,
}
impl ColumnRename {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        requested: &TableName,
        from: &ColumnName,
        to: &ColumnName,
        if_exists: bool,
    ) -> Result<(Option<TableName>, Self), AnalysisError> {
        let target = context.resolve_table_name(requested).map(|target| {
            if context.dialect() == Dialect::sqlite() {
                TableName::from(target.full_name().sqlite_identity())
            } else {
                target
            }
        });
        let table = target.as_ref().unwrap_or(requested);
        let definition = if let Some(name) = &target {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        let sqlite = context.dialect() == Dialect::sqlite();
        let (binding, duplicate) = if let Some(table) = &definition {
            (
                table.find_column(from, sqlite),
                table.find_column(to, sqlite),
            )
        } else {
            (None, None)
        };
        let permitted =
            duplicate.is_none() || (sqlite && binding.is_some() && binding == duplicate);
        if target.is_none() {
            if !if_exists
                || context
                    .resolve_relation_name(requested.full_name())
                    .is_some()
            {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            }
        } else if binding.is_none() {
            context
                .report_analysis_problem(AnalysisProblem::column_not_found_in_table(table, from))?;
        } else if !permitted {
            context.report_analysis_problem(AnalysisProblem::column_already_exists(table, to))?;
        }
        let mut rename = Self::new(table, from, binding, to, permitted, sqlite);
        rename.validate_dependencies(context)?;
        Ok((target, rename))
    }

    pub(crate) fn validate_dependencies(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.applicable && context.dialect() == Dialect::sqlite() {
            let problems = self.sqlite_dependency_problems(context)?;
            self.applicable = problems.is_empty();
            for problem in problems {
                context.report_analysis_problem(problem)?;
            }
        }
        Ok(())
    }

    pub(crate) fn new(
        table: &TableName,
        requested: &ColumnName,
        source: Option<&ColumnDefinition>,
        destination: &ColumnName,
        permitted: bool,
        rewrites_outputs: bool,
    ) -> Self {
        Self {
            table: table.clone(),
            requested: requested.clone(),
            source: source.cloned(),
            destination: destination.clone(),
            applicable: source.is_some() && permitted,
            rewrites_outputs,
            sqlite_outputs: std::collections::HashMap::new(),
        }
    }
    /// Target identity used by this operation.
    ///
    /// When relation lookup failed this is the requested name; the enclosing
    /// `AlterTable::resolved_target` remains `None`, and application is prohibited.
    pub const fn table(&self) -> &TableName {
        &self.table
    }
    /// Old name supplied by the statement.
    pub const fn requested(&self) -> &ColumnName {
        &self.requested
    }
    /// Completed source-column binding, absent if lookup failed.
    pub const fn source(&self) -> Option<&ColumnDefinition> {
        self.source.as_ref()
    }
    /// Requested new name.
    pub const fn destination(&self) -> &ColumnName {
        &self.destination
    }
    /// Whether analysis permits application.
    pub const fn can_apply(&self) -> bool {
        self.applicable
    }
    pub(crate) const fn rewrites_outputs(&self) -> bool {
        self.rewrites_outputs
    }
    pub(crate) fn matches_table(&self, table: &TableName) -> bool {
        table == &self.table
    }
    pub(crate) fn rename_name(&self, name: &mut ColumnName) {
        if let Some(column) = &self.source
            && let Some(source) = column.name()
            && (if self.rewrites_outputs {
                source.as_str().eq_ignore_ascii_case(name.as_str())
            } else {
                source == name
            })
        {
            *name = self.destination.clone();
        }
    }
    pub(crate) fn rename_names(&self, names: &mut [ColumnName]) {
        for name in names {
            self.rename_name(name);
        }
    }
}
