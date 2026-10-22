use crate::sem::{
    AliasName, AnalysisError, ColumnName, DataSource, FullName, ParseContext, SemScalarExpr,
    WithClause, data_source::MergeRecordRole,
};

/// A visible source occurrence, selected without a second lookup by bare alias.
#[derive(Debug, Clone)]
pub(crate) struct QualifiedSource<'a> {
    source: &'a DataSource,
    alias: AliasName,
    nullable: bool,
    merge_role: Option<MergeRecordRole>,
}

impl<'a> QualifiedSource<'a> {
    pub(super) fn new(source: &'a DataSource, alias: &AliasName) -> Self {
        Self {
            source,
            alias: alias.clone(),
            nullable: false,
            merge_role: None,
        }
    }

    pub(super) const fn with_nullability(mut self, nullable: bool) -> Self {
        self.nullable |= nullable;
        self
    }

    pub(super) const fn with_merge(mut self, role: MergeRecordRole, nullable: bool) -> Self {
        self.nullable |= nullable;
        self.merge_role = Some(role);
        self
    }

    pub(crate) const fn alias(&self) -> &AliasName {
        &self.alias
    }

    pub(crate) fn projection_columns(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<super::ProjectionColumn<'a>>, AnalysisError> {
        Ok(self
            .source
            .projection_names(context, with, Some(&self.alias))?
            .into_iter()
            .map(|column| column.with_nullability(self.nullable))
            .collect())
    }

    pub(super) fn column_candidates(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<super::ColumnCandidates, AnalysisError> {
        self.source
            .column_occurrence_candidates(context, with, column, &self.alias)
    }

    pub(crate) fn bind(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let canonical;
        let column = if context.dialect() == crate::ast::Dialect::sqlite() {
            canonical = self.source.canonical_relation_column(context, column);
            canonical.as_ref().unwrap_or(column)
        } else {
            column
        };
        let value = self
            .source
            .create_column_ref(context, with, &self.alias, column, self.nullable)?
            .0;
        Ok(if let Some(role) = self.merge_role {
            value.merge_record_column(role, self.nullable)
        } else {
            value
        })
    }
}

impl DataSource {
    fn column_occurrence_candidates(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
        alias: &AliasName,
    ) -> Result<super::ColumnCandidates, AnalysisError> {
        if let Self::JoinExpr(join) = self {
            if join.column_candidates(column, context, with).is_unknown() {
                return Ok(super::ColumnCandidates::unknown());
            }
            let mut aliases = Vec::new();
            for projected in join.project(context, with, None)? {
                if projected.name() == column {
                    aliases.push(super::AliasSpec::from_local_name(alias.as_str()));
                }
            }
            return Ok(super::ColumnCandidates::from(aliases));
        }
        Ok(self.aliases_for_column_resolved(column, context, with))
    }

    pub(super) fn sqlite_qualified_group_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
        alias: &'a super::AliasSpec,
    ) -> Option<QualifiedSource<'a>> {
        match self {
            Self::Table(table) => table.qualified_source(self, context, name, Some(alias)),
            Self::View(view) => view.qualified_source(self, context, name, Some(alias)),
            Self::Group(group) => group.sqlite_qualified_group_source(context, name, alias),
            _ => None,
        }
    }

    pub(crate) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<QualifiedSource<'a>>, ()> {
        match self {
            Self::Table(table) => Ok(table.qualified_source(self, context, name, None)),
            Self::View(view) => Ok(view.qualified_source(self, context, name, None)),
            Self::JoinExpr(join) => join.qualified_source(context, name),
            Self::Group(group) => group.qualified_source(context, name),
            Self::MergeRecord(record) => record.qualified_source(context, name),
            _ => Ok(None),
        }
    }
}

pub(super) fn relation_matches(
    context: &impl ParseContext,
    name: &FullName,
    relation: &FullName,
    alias: &super::AliasSpec,
) -> bool {
    if context.dialect() == crate::ast::Dialect::sqlite() {
        name.schema()
            .zip(relation.schema())
            .is_some_and(|(requested, actual)| {
                requested.as_str().eq_ignore_ascii_case(actual.as_str())
            })
            && name
                .name()
                .as_str()
                .eq_ignore_ascii_case(alias.name().as_str())
    } else {
        !alias.is_explicit() && name == relation
    }
}
