//! Creation-time foreign-key bindings, independent of subsequent search paths.
use crate::{
    context::ParseContext,
    sem::{AnalysisError, AnalysisProblem, ColumnName, TableName, TypeReference},
};

/// A completed target lookup: `None` means the relation was unresolved.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct ForeignKeyTarget(Option<TableName>);

impl ForeignKeyTarget {
    pub(crate) fn new(canonical: Option<&TableName>) -> Self {
        Self(canonical.cloned())
    }

    pub(crate) const fn relation(&self) -> Option<&TableName> {
        self.0.as_ref()
    }

    fn resolve(context: &impl ParseContext, requested: &TableName) -> Self {
        let canonical = context.resolve_table_name(requested).map(|name| {
            if context.dialect() == crate::ast::Dialect::sqlite() {
                TableName::from(name.full_name().sqlite_identity())
            } else {
                name
            }
        });
        Self(canonical)
    }

    pub(crate) fn resolve_relation(
        context: &mut impl ParseContext,
        requested: &TableName,
    ) -> Result<Self, AnalysisError> {
        let target = Self::resolve(context, requested);
        if target.relation().is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
        }
        Ok(target)
    }

    pub(crate) fn resolve_column(
        context: &mut impl ParseContext,
        requested: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, Option<TypeReference>), AnalysisError> {
        let target = Self::resolve(context, requested);
        let Some(relation) = target.relation() else {
            context.report_analysis_problem(AnalysisProblem::table_not_found(requested))?;
            return Ok((target, None));
        };
        let definition = context.get_table_impl(relation.full_name());
        let column_type = if let Some(table) = definition
            && let Some(column) = table.get_column_def(column)
        {
            column.get_type_cloned()
        } else {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                requested, column,
            ))?;
            None
        };
        Ok((target, column_type))
    }

    pub(crate) fn referenced_columns(
        &self,
        context: &mut impl ParseContext,
        requested: &TableName,
        explicit: Option<&[ColumnName]>,
    ) -> Result<Option<Vec<ColumnName>>, AnalysisError> {
        let definition = if let Some(name) = self.relation() {
            context.get_table_impl(name.full_name())
        } else {
            None
        };
        let columns = if let Some(columns) = explicit {
            Some(columns.to_vec())
        } else {
            definition.map(|table| table.primary_key_columns().into_iter().cloned().collect())
        };
        if let Some(relation) = self.relation()
            && let Some(columns) = &columns
        {
            for column in columns {
                if !context
                    .get_table_impl(relation.full_name())
                    .is_some_and(|table| table.has_column(column))
                {
                    context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                        requested, column,
                    ))?;
                }
            }
        }
        Ok(columns)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(relation) = &mut self.0 {
            relation.rename_reference(old, new);
        }
    }
}
