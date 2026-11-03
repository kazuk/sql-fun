use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, DataSource, Nullable, SemScalarExpr, WithClause,
        data_source::{
            ColumnCandidates, ProjectionColumn, QualifiedSource, SourceNamespace,
            projection::ProjectionColumns,
        },
        scalar_expr::ColumnReferenceExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A SQLite parenthesized source, retaining its original column bindings.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupedDataSource {
    source: DataSource,
    alias: AliasSpec,
}

impl GroupedDataSource {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.source.projection_shape_is_resolved()
    }

    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.source.projection_is_table_dependent(context)
    }

    pub(super) fn sqlite_qualified_group_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
        alias: &'a AliasSpec,
    ) -> Option<QualifiedSource<'a>> {
        self.source
            .sqlite_qualified_group_source(context, name, alias)
    }

    pub(super) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<QualifiedSource<'a>>, ()> {
        if context.dialect() != Dialect::sqlite() {
            return Ok(None);
        }
        if self.is_joined() {
            self.source.qualified_source(context, name)
        } else {
            Ok(self
                .source
                .sqlite_qualified_group_source(context, name, &self.alias))
        }
    }

    pub(super) fn namespace<'a>(&'a self, source: &'a DataSource) -> SourceNamespace<'a> {
        let mut namespace = SourceNamespace::default();
        namespace.push(source, &self.alias);
        if self.is_joined() {
            namespace.hide_group_columns();
            namespace.append(self.source.namespace());
        }
        namespace
    }

    pub(super) fn new(source: &DataSource, alias: &AliasSpec) -> Self {
        Self {
            source: source.clone(),
            alias: alias.clone(),
        }
    }

    pub(super) fn projection_names<'a>(
        &'a self,
        source: &'a DataSource,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ProjectionColumn<'a>>, AnalysisError> {
        Ok(self
            .source
            .projection_names(context, with, None)?
            .into_iter()
            .map(|column| column.with_group(source, self.alias.name()))
            .collect())
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

    pub(super) fn column_candidates<C: ParseContext>(
        &self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> ColumnCandidates {
        let candidates = self
            .source
            .aliases_for_column_resolved(column, context, with);
        if self.is_joined() {
            candidates
        } else {
            candidates.with_group_alias(&self.alias)
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
        for projected in self.source.projection_names(context, with, None)? {
            if projected.name() == column && projected.is_merged() {
                return Ok((projected.bind_direct(context, with)?, context));
            }
        }
        let Some((source, source_nullable)) = self.column_source(column, &*context, with) else {
            let expr = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
            return Ok((expr, context));
        };
        source.create_column_ref(context, with, alias, column, nullable || source_nullable)
    }
}

impl GroupedDataSource {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.source.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.source.visit(visitor)?;
        Ok(())
    }
}

impl GroupedDataSource {
    pub(super) fn projection_column_names(
        &self,
        source: &DataSource,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        let columns = self.projection_names(source, context, with)?;
        Ok(ProjectionColumns::from(columns).names())
    }
}
