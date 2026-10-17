use crate::sem::{
    AliasSpec, AnalysisError, ColumnName, DataSource, Nullable, ParseContext, SemScalarExpr,
    WithClause, data_source::AliasName, scalar_expr::ColumnReferenceExpr,
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

    pub(super) fn get_aliases_for_column<C: ParseContext>(
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

    pub(super) fn column_source<'a, C: ParseContext>(
        &'a self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> Option<(&'a DataSource, Nullable)> {
        self.source.group_column_source(column, context, with)
    }

    pub(super) fn create_column_ref<'context, C: ParseContext>(
        &self,
        context: &'context mut C,
        with: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullable: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut C), AnalysisError> {
        let Some((source, source_nullable)) = self.column_source(column, &*context, with) else {
            let expr = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
            return Ok((expr, context));
        };
        source.create_column_ref(context, with, alias, column, nullable || source_nullable)
    }
}

impl GroupedDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.source.walk(visitor)?;
        Ok(())
    }
}
