use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnName, DataSource, Nullable, TableName,
        data_source::AliasName,
        scalar_expr::{ColumnReferenceExpr, SystemColumnRef},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableDataSource {
    name: TableName,
    alias: AliasSpec,
}

impl TableDataSource {
    pub(super) fn qualified_source<'a>(
        &'a self,
        source: &'a DataSource,
        context: &impl ParseContext,
        name: &crate::sem::FullName,
        alias: Option<&'a AliasSpec>,
    ) -> Option<super::QualifiedSource<'a>> {
        let alias = alias.unwrap_or(&self.alias);
        if super::qualified::relation_matches(context, name, self.name.full_name(), alias) {
            Some(super::QualifiedSource::new(source, alias.name()))
        } else {
            None
        }
    }

    pub(super) fn new_data_source(name: &TableName, alias: &AliasSpec) -> DataSource {
        DataSource::Table(Self {
            name: name.clone(),
            alias: alias.clone(),
        })
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        if let Some(table) = context.get_table(&self.name)
            && (self
                .alias
                .derived_column_index(column)
                .is_some_and(|index| table.get_column_at(index).is_some())
                || table
                    .get_column_def_with_skip_count(column, self.alias.len_derived_column())
                    .is_some()
                || SystemColumnRef::lookup_type(context, column).is_some())
        {
            names.push(self.alias.clone());
        }
    }

    pub const fn name(&self) -> &TableName {
        &self.name
    }

    pub(super) fn create_column_ref<TParseContext>(
        &self,
        context: &mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<ColumnReferenceExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(table) = context.get_table(&self.name) else {
            AnalysisError::raise_unexpected_input(&format!(
                "table {} not found in context",
                self.name
            ))?;
            AnalysisError::never()
        };

        let column_def = if let Some(index) = self.alias.derived_column_index(column) {
            if let Some(column_def) = table.get_column_at(index) {
                Some(column_def)
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::table_not_have_column_for_derived_column_name(),
                )?;
                None
            }
        } else if let Some(column_def) =
            table.get_column_def_with_skip_count(column, self.alias.len_derived_column())
        {
            Some(column_def)
        } else if let Some(column_type) = SystemColumnRef::lookup_type(context, column) {
            return Ok(ColumnReferenceExpr::system_column(
                alias,
                column,
                &self.name,
                &column_type,
                nullability,
            ));
        } else {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                &self.name, column,
            ))?;
            None
        };
        Ok(ColumnReferenceExpr::table_column(
            alias,
            column,
            &self.name,
            column_def,
            nullability,
        ))
    }
}

#[cfg(test)]
mod tests;

impl TableDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        visitor.relation_identity(self.name.full_name());
        visitor.relation(None, self.alias.name().as_str(), &[]);
        Ok(())
    }
}

impl TableDataSource {
    pub(super) fn typed_projection_record(
        &self,
        alias: &AliasName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<crate::sem::WildcardTypedRecord> {
        crate::sem::WildcardTypedRecord::from_table(alias, &self.name, context, index)
    }

    pub(super) const fn wildcard_relation(&self) -> &crate::sem::FullName {
        self.name.full_name()
    }

    pub(super) fn unaliased_projection_relation(
        &self,
        alias: &AliasName,
    ) -> Option<&crate::sem::FullName> {
        if !self.alias.is_explicit() && self.alias.name() == alias {
            Some(self.name.full_name())
        } else {
            None
        }
    }

    pub(super) fn bind_projection_ordinal(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
        column: &ColumnName,
        index: usize,
        nullable: bool,
    ) -> crate::sem::SemScalarExpr {
        let definition = if let Some(table) = context.get_table(&self.name) {
            table.get_column_at(index)
        } else {
            None
        };
        crate::sem::SemScalarExpr::column_reference(ColumnReferenceExpr::table_column(
            alias, column, &self.name, definition, nullable,
        ))
    }
}

impl TableDataSource {
    pub(super) fn projection_column_names(
        &self,
        context: &impl ParseContext,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        let Some(table) = context.get_table(&self.name) else {
            return AnalysisError::raise_unexpected_input("star relation missing");
        };
        Ok(table.column_names())
    }
}
