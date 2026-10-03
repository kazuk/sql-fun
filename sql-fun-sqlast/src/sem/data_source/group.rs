use crate::sem::{
    AliasSpec, AnalysisError, ColumnName, DataSource, Nullable, SemScalarExpr, WithClause,
    data_source::AliasName, resolution::ResolutionContext, scalar_expr::ColumnReferenceExpr,
};

/// A SQLite parenthesized source, retaining its original column bindings.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupedDataSource {
    source: DataSource,
    alias: AliasSpec,
}

impl GroupedDataSource {
    pub(super) fn new(source: &DataSource, alias: &AliasSpec) -> Self {
        Self {
            source: source.clone(),
            alias: alias.clone(),
        }
    }

    pub(super) fn is_joined(&self) -> bool {
        self.source.is_joined()
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
        if self.is_joined() {
            self.source.push_alias_names(names);
        }
    }

    pub(super) fn resolve_alias<'a>(
        &'a self,
        source: &'a DataSource,
        alias: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == alias {
            Some((source, false))
        } else if self.is_joined() {
            self.source.resolve_alias(alias)
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<C: ResolutionContext>(
        &self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) {
        let aliases = self
            .source
            .aliases_for_column_resolved(column, context, with);
        if self.is_joined() {
            names.extend(aliases);
        } else if !aliases.is_empty() {
            names.push(self.alias.clone());
        }
    }

    pub(super) fn column_source<'a, C: ResolutionContext>(
        &'a self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> Option<(&'a DataSource, Nullable)> {
        self.source.group_column_source(column, context, with)
    }

    pub(super) fn create_column_ref<C: ResolutionContext>(
        &self,
        mut context: C,
        with: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullable: Nullable,
    ) -> Result<(SemScalarExpr, C), AnalysisError> {
        let Some((source, source_nullable)) = self.column_source(column, &context, with) else {
            let expr = ColumnReferenceExpr::undefined_column_resolved(&mut context, column)?;
            return Ok((expr, context));
        };
        source.create_column_ref(context, with, alias, column, nullable || source_nullable)
    }
}
