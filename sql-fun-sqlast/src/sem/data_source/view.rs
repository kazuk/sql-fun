use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName, FullName, ViewName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnDefinition, ColumnReferenceExpr,
        DataSource, Nullable, ResultColumn, SemScalarExpr, ViewLike, WildcardTypedRecord,
        data_source::{QualifiedSource, qualified::relation_matches},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewDataSource {
    view: ViewName,
    alias: AliasSpec,
}

impl ViewDataSource {
    pub(super) fn qualified_source<'a>(
        &'a self,
        source: &'a DataSource,
        context: &impl ParseContext,
        name: &FullName,
        alias: Option<&'a AliasSpec>,
    ) -> Option<QualifiedSource<'a>> {
        let alias = alias.unwrap_or(&self.alias);
        if relation_matches(context, name, self.view.full_name(), alias) {
            Some(QualifiedSource::new(source, alias.name()))
        } else {
            None
        }
    }

    fn result_column<'a>(
        &self,
        view: &'a ViewLike,
        column: &ColumnName,
    ) -> Option<&'a ResultColumn> {
        let columns = view.result_columns();
        if let Some(index) = self.alias.derived_column_index(column) {
            columns.get_column_at(index)
        } else {
            columns.get_column_with_skip_count(column, self.alias.len_derived_column())
        }
    }
    fn column_definition(&self, view: &ViewLike, column: &ColumnName) -> Option<ColumnDefinition> {
        self.result_column(view, column)
            .map(ResultColumn::create_column_definition)
    }
    pub(super) fn create_column_ref(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<ColumnReferenceExpr, AnalysisError> {
        let definition = match context.get_view(&self.view) {
            Some(view) => self.column_definition(view, column),
            None => None,
        };
        if definition.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_view(
                &self.view, column,
            ))?;
        }
        Ok(ColumnReferenceExpr::view_column(
            alias,
            column,
            &self.view,
            definition.as_ref(),
            nullability,
        ))
    }

    pub(super) fn from_name(view: &ViewName, alias: &AliasSpec) -> Self {
        Self {
            view: view.clone(),
            alias: alias.clone(),
        }
    }

    pub const fn view_name(&self) -> &ViewName {
        &self.view
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
        if let Some(view) = context.get_view(&self.view)
            && self.result_column(view, column).is_some()
        {
            names.push(self.alias.clone());
        }
    }
}

impl ViewDataSource {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.source_binding(self.view.full_name(), &self.alias);
        visitor.relation_identity(self.view.full_name());
        visitor.relation(None, self.alias.name().as_str(), &[]);
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.source_binding(self.view.full_name(), &self.alias);
        visitor.relation_identity(self.view.full_name());
        visitor.relation(None, self.alias.name().as_str(), &[]);
        Ok(())
    }
}

impl ViewDataSource {
    pub(super) fn typed_projection_record(
        &self,
        alias: &AliasName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<WildcardTypedRecord> {
        WildcardTypedRecord::from_view(alias, &self.view, context, index)
    }

    pub(super) const fn wildcard_relation(&self) -> &FullName {
        self.view.full_name()
    }

    pub(super) fn unaliased_projection_relation(&self, alias: &AliasName) -> Option<&FullName> {
        if !self.alias.is_explicit() && self.alias.name() == alias {
            Some(self.view.full_name())
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
    ) -> SemScalarExpr {
        let definition = if let Some(view) = context.get_view(&self.view) {
            view.result_columns()
                .get_column_at(index)
                .map(ResultColumn::create_column_definition)
        } else {
            None
        };
        SemScalarExpr::column_reference(ColumnReferenceExpr::view_column(
            alias,
            column,
            &self.view,
            definition.as_ref(),
            nullable,
        ))
    }
}

impl ViewDataSource {
    pub(super) fn projection_column_names(
        &self,
        context: &impl ParseContext,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        let Some(view) = context.get_view(&self.view) else {
            return AnalysisError::raise_unexpected_none("star view");
        };
        Ok(view.result_columns().names())
    }
}
