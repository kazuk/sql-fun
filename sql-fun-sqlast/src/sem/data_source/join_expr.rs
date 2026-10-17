use crate::sem::{
    AliasSpec, ColumnName, DataSource, JoinKind, Nullable, SemScalarExpr, WithClause,
    data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinUsingClause {
    using_columns: Vec<ColumnName>,
    alias: Option<AliasSpec>,
}

impl JoinUsingClause {
    pub(crate) fn new(using_columns: &[ColumnName], alias: &Option<AliasSpec>) -> Self {
        Self {
            using_columns: using_columns.to_owned(),
            alias: alias.clone(),
        }
    }

    fn contains_column(&self, column: &ColumnName) -> bool {
        self.using_columns.contains(column)
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinExprDataSource {
    kind: JoinKind,
    left: DataSource,
    right: DataSource,
    qualifications: Option<SemScalarExpr>,
    alias: Option<AliasSpec>,
    using_clause: Option<JoinUsingClause>,
    natural: bool,
}

impl JoinExprDataSource {
    pub(super) fn new_data_source(
        kind: &JoinKind,
        left: &DataSource,
        right: &DataSource,
        qualifications: &Option<SemScalarExpr>,
        alias: &Option<AliasSpec>,
        using_clause: &Option<JoinUsingClause>,
        natural: bool,
    ) -> DataSource {
        DataSource::JoinExpr(Box::new(Self {
            kind: kind.clone(),
            left: left.clone(),
            right: right.clone(),
            qualifications: qualifications.clone(),
            alias: alias.clone(),
            using_clause: using_clause.clone(),
            natural,
        }))
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        self.left.push_alias_names(names);
        self.right.push_alias_names(names);
    }

    pub(super) fn resolve_alias<'a>(
        &'a self,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if let Some((left_source, left_nullable)) = self.left.resolve_alias(name) {
            Some((left_source, self.kind.left_nullable(left_nullable)))
        } else {
            let (right_source, right_nullable) = self.right.resolve_alias(name)?;
            Some((right_source, self.kind.right_nullable(right_nullable)))
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: crate::sem::ParseContext,
    {
        let mut left_aliases = self
            .left
            .aliases_for_column_resolved(column, context, with_clause);
        let mut right_aliases =
            self.right
                .aliases_for_column_resolved(column, context, with_clause);
        names.append(&mut left_aliases);
        names.append(&mut right_aliases);
    }
}

impl JoinExprDataSource {
    pub(super) fn group_column_source<'a, C: crate::sem::ParseContext>(
        &'a self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> Option<(&'a DataSource, Nullable)> {
        let left = self.left.group_column_source(column, context, with);
        // RIGHT JOIN preserves the right-hand USING or NATURAL column.
        if self.kind.uses_right_join_column()
            && left.is_some()
            && (self.natural
                || self
                    .using_clause
                    .as_ref()
                    .is_some_and(|using| using.contains_column(column)))
            && let Some((source, nullable)) = self.right.group_column_source(column, context, with)
        {
            return Some((source, self.kind.right_nullable(nullable)));
        }
        // Other duplicate column names select the first matching column.
        if let Some((source, nullable)) = left {
            Some((source, self.kind.left_nullable(nullable)))
        } else {
            let (source, nullable) = self.right.group_column_source(column, context, with)?;
            Some((source, self.kind.right_nullable(nullable)))
        }
    }
}

impl JoinExprDataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.left.walk(visitor)?;
        self.right.walk(visitor)?;
        crate::sem::traversal::optional(&mut self.qualifications, visitor)?;
        if let Some(value) = &self.qualifications {
            visitor.predicate(value);
        }
        Ok(())
    }
}

impl JoinExprDataSource {
    pub(super) fn matches_projection_qualifier(&self, qualifier: Option<&AliasName>) -> bool {
        qualifier.is_none_or(|name| {
            self.alias
                .as_ref()
                .is_some_and(|alias| alias.name() == name)
        })
    }

    pub(super) fn projection_names(
        &self,
        context: &impl crate::sem::ParseContext,
        with: &WithClause,
    ) -> Result<Vec<(AliasName, ColumnName)>, crate::sem::AnalysisError> {
        let left = self.left.projection_names(context, with, None)?;
        let right = self.right.projection_names(context, with, None)?;
        let merged = if let Some(using) = &self.using_clause {
            using.using_columns.clone()
        } else if self.natural {
            left.iter()
                .filter(|(_, name)| right.iter().any(|(_, other)| name == other))
                .map(|(_, name)| name.clone())
                .collect()
        } else {
            Vec::new()
        };
        if merged.is_empty() {
            return Ok(left.into_iter().chain(right).collect());
        }
        self.kind.require_projectable_merged_columns();
        let mut columns = Vec::new();
        if context.dialect() == crate::ast::Dialect::postgres() {
            for name in &merged {
                let source = if self.kind.uses_right_join_column() {
                    &right
                } else {
                    &left
                };
                if let Some(column) = source.iter().find(|(_, column)| column == name) {
                    columns.push(column.clone());
                }
            }
            columns.extend(left.into_iter().filter(|(_, name)| !merged.contains(name)));
        } else {
            columns.extend(left);
        }
        columns.extend(right.into_iter().filter(|(_, name)| !merged.contains(name)));
        Ok(columns)
    }
}
