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

    fn canonical_relation(context: &impl ParseContext, name: TableName) -> TableName {
        if context.dialect() == crate::ast::Dialect::sqlite() {
            TableName::from(name.full_name().sqlite_identity())
        } else {
            name
        }
    }

    fn resolve(context: &impl ParseContext, requested: &TableName) -> Self {
        Self(
            context
                .resolve_table_name(requested)
                .map(|name| Self::canonical_relation(context, name)),
        )
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

    pub(crate) fn resolve_declared_reference(
        context: &mut impl ParseContext,
        requested: &TableName,
        explicit: Option<&[ColumnName]>,
        pending: &crate::sem::CreateTable,
    ) -> Result<(Self, Option<Vec<ColumnName>>, bool), AnalysisError> {
        let identity = context.creation_name(pending.name().full_name(), pending.temporary())?;
        let lookup = if context.dialect() == crate::ast::Dialect::sqlite() {
            let Some(schema) = identity.schema() else {
                AnalysisError::raise_unexpected_none("SQLite creation schema")?;
                AnalysisError::never();
            };
            requested.full_name().replace_schema(schema)
        } else {
            requested.full_name().clone()
        };
        let canonical =
            crate::sem::search_path::expand_relation_search_path(context, &lookup, |key| {
                let matches = if context.dialect() == crate::ast::Dialect::sqlite() {
                    key.sqlite_eq(&identity)
                } else {
                    key == &identity
                };
                if matches {
                    Some(TableName::from(identity.clone()))
                } else {
                    context.resolve_table_name(&TableName::from(key.clone()))
                }
            });
        let target = Self(canonical.map(|name| Self::canonical_relation(context, name)));
        if target.relation().is_none() && context.dialect() == crate::ast::Dialect::postgres() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
        }
        let (columns, valid) = if target.references_identity(context, &identity) {
            let (columns, valid) =
                pending.resolve_reference_columns(context, requested, explicit)?;
            (Some(columns), valid)
        } else {
            target.referenced_columns(context, requested, explicit)?
        };
        Ok((target, columns, valid))
    }

    fn references_identity(
        &self,
        context: &impl ParseContext,
        identity: &crate::sem::FullName,
    ) -> bool {
        self.relation().is_some_and(|name| {
            if context.dialect() == crate::ast::Dialect::sqlite() {
                name.full_name().sqlite_eq(identity)
            } else {
                name.full_name() == identity
            }
        })
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
    ) -> Result<(Option<Vec<ColumnName>>, bool), AnalysisError> {
        let definition = if let Some(name) = self.relation() {
            context.get_table_impl(name.full_name()).cloned()
        } else {
            None
        };
        if let Some(definition) = definition {
            let (columns, valid) =
                definition.resolve_reference_columns(context, requested, explicit)?;
            Ok((Some(columns), valid))
        } else {
            Ok((
                explicit.map(<[ColumnName]>::to_vec),
                context.dialect() == crate::ast::Dialect::sqlite(),
            ))
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(relation) = &mut self.0 {
            relation.rename_reference(old, new);
        }
    }
}
