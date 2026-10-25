use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, ColumnName, DataSource, FullName, JoinKind, Nullable, SemScalarExpr, WithClause,
        data_source::AliasName,
    },
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
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.left.projection_shape_is_resolved() && self.right.projection_shape_is_resolved()
    }

    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.left.projection_is_table_dependent(context)
            || self.right.projection_is_table_dependent(context)
    }

    pub(super) fn namespace<'a>(&'a self, source: &'a DataSource) -> super::SourceNamespace<'a> {
        if let Some(alias) = &self.alias {
            let mut namespace = super::SourceNamespace::default();
            namespace.push(source, alias);
            return namespace;
        }
        let mut namespace = self
            .left
            .namespace()
            .with_nullability(self.kind.left_nullable(false));
        namespace.append(
            self.right
                .namespace()
                .with_nullability(self.kind.right_nullable(false)),
        );
        namespace
    }

    pub(super) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<super::QualifiedSource<'a>>, ()> {
        if self.alias.is_some() {
            return Ok(None);
        }
        let left = self.left.qualified_source(context, name)?;
        let right = self.right.qualified_source(context, name)?;
        match (left, right) {
            (Some(_), Some(_)) => Err(()),
            (Some(source), None) => Ok(Some(
                source.with_nullability(self.kind.left_nullable(false)),
            )),
            (None, Some(source)) => Ok(Some(
                source.with_nullability(self.kind.right_nullable(false)),
            )),
            (None, None) => Ok(None),
        }
    }

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
        if let Some(alias) = &self.alias {
            names.push(alias.clone());
        } else {
            self.left.push_alias_names(names);
            self.right.push_alias_names(names);
        }
    }

    pub(super) fn resolve_alias<'a>(
        &'a self,
        source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if let Some(alias) = &self.alias {
            return if alias.name() == name {
                Some((source, false))
            } else {
                None
            };
        }
        if let Some((left_source, left_nullable)) = self.left.resolve_alias(name) {
            Some((left_source, self.kind.left_nullable(left_nullable)))
        } else {
            let (right_source, right_nullable) = self.right.resolve_alias(name)?;
            Some((right_source, self.kind.right_nullable(right_nullable)))
        }
    }

    pub(super) fn column_candidates<TParseContext: ParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> super::ColumnCandidates {
        let mut candidates = self
            .left
            .aliases_for_column_resolved(column, context, with_clause);
        candidates.append(
            self.right
                .aliases_for_column_resolved(column, context, with_clause),
        );
        candidates
    }
}

impl JoinExprDataSource {
    pub(super) fn create_column_ref<'context, C: ParseContext>(
        &self,
        context: &'context mut C,
        with: &WithClause,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'context mut C), crate::sem::AnalysisError> {
        if self.column_candidates(column, context, with).is_unknown() {
            let expression =
                crate::sem::ColumnReferenceExpr::undefined_column_resolved(context, column)?;
            return Ok((expression, context));
        }
        let columns = self.project(context, with, None)?;
        let mut selected = None;
        for candidate in columns {
            if candidate.name() == column {
                if selected.is_some() {
                    return Ok((
                        crate::sem::ColumnReferenceExpr::ambiguous_column_resolved(
                            context, column,
                        )?,
                        context,
                    ));
                }
                selected = Some(candidate);
            }
        }
        let value = if let Some(selected) = selected {
            selected
                .with_nullability(nullable)
                .bind_direct(context, with)?
        } else {
            crate::sem::ColumnReferenceExpr::undefined_column_resolved(context, column)?
        };
        Ok((value, context))
    }

    pub(super) fn group_column_source<'a, C: ParseContext>(
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
    pub(super) fn project(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<super::ProjectionColumn<'_>>, crate::sem::AnalysisError> {
        if self.matches_projection_qualifier(qualifier) {
            let columns = self.projection_names(context, with)?;
            return Ok(if let Some(alias) = &self.alias {
                super::projection::ProjectionColumns::from(columns)
                    .with_join_alias(alias)
                    .into_columns()
            } else {
                columns
            });
        }
        if self.alias.is_some() {
            return Ok(Vec::new());
        }
        let mut columns = Vec::new();
        columns.extend(
            self.left
                .projection_names(context, with, qualifier)?
                .into_iter()
                .map(|column| column.with_nullability(self.kind.left_nullable(false))),
        );
        columns.extend(
            self.right
                .projection_names(context, with, qualifier)?
                .into_iter()
                .map(|column| column.with_nullability(self.kind.right_nullable(false))),
        );
        Ok(columns)
    }

    pub(super) fn matches_projection_qualifier(&self, qualifier: Option<&AliasName>) -> bool {
        qualifier.is_none_or(|name| {
            self.alias
                .as_ref()
                .is_some_and(|alias| alias.name() == name)
        })
    }

    pub(super) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<super::ProjectionColumn<'_>>, crate::sem::AnalysisError> {
        let original_left = self.left.projection_names(context, with, None)?;
        let original_right = self.right.projection_names(context, with, None)?;
        let merged = self.merged_projection_names(&original_left, &original_right);
        let left = original_left
            .iter()
            .cloned()
            .map(|column| column.with_nullability(self.kind.left_nullable(false)))
            .collect::<Vec<_>>();
        let right = original_right
            .iter()
            .cloned()
            .map(|column| column.with_nullability(self.kind.right_nullable(false)))
            .collect::<Vec<_>>();
        if merged.is_empty() {
            return Ok(left.into_iter().chain(right).collect());
        }
        if context.dialect() == crate::ast::Dialect::postgres() {
            Ok(self.postgres_projection(&original_left, &original_right, left, right, &merged))
        } else {
            Ok(self.sqlite_projection(&original_left, &original_right, left, right, &merged))
        }
    }

    fn merged_projection_names(
        &self,
        left: &[super::ProjectionColumn<'_>],
        right: &[super::ProjectionColumn<'_>],
    ) -> Vec<ColumnName> {
        if let Some(using) = &self.using_clause {
            using.using_columns.clone()
        } else if self.natural {
            left.iter()
                .filter(|column| right.iter().any(|other| column.name() == other.name()))
                .map(|column| column.name().clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn full_join_key<'a>(
        name: &ColumnName,
        left: &[super::ProjectionColumn<'a>],
        right: &[super::ProjectionColumn<'a>],
    ) -> Option<super::ProjectionColumn<'a>> {
        let left = left.iter().find(|column| column.name() == name)?;
        let right = right.iter().find(|column| column.name() == name)?;
        Some(super::ProjectionColumn::merged(left, right))
    }

    fn postgres_projection<'a>(
        &self,
        original_left: &[super::ProjectionColumn<'a>],
        original_right: &[super::ProjectionColumn<'a>],
        left: Vec<super::ProjectionColumn<'a>>,
        right: Vec<super::ProjectionColumn<'a>>,
        merged: &[ColumnName],
    ) -> Vec<super::ProjectionColumn<'a>> {
        let mut columns = Vec::new();
        for name in merged {
            let key = if self.kind.is_full() {
                Self::full_join_key(name, original_left, original_right)
            } else {
                let source = if self.kind.uses_right_join_column() {
                    &right
                } else {
                    &left
                };
                source.iter().find(|column| column.name() == name).cloned()
            };
            if let Some(key) = key {
                columns.push(key);
            }
        }
        columns.extend(
            left.into_iter()
                .filter(|column| !merged.contains(column.name())),
        );
        columns.extend(
            right
                .into_iter()
                .filter(|column| !merged.contains(column.name())),
        );
        columns
    }

    fn sqlite_projection<'a>(
        &self,
        original_left: &[super::ProjectionColumn<'a>],
        original_right: &[super::ProjectionColumn<'a>],
        left: Vec<super::ProjectionColumn<'a>>,
        right: Vec<super::ProjectionColumn<'a>>,
        merged: &[ColumnName],
    ) -> Vec<super::ProjectionColumn<'a>> {
        let mut columns = Vec::new();
        for column in left {
            if self.kind.is_full() && merged.contains(column.name()) {
                if let Some(key) = Self::full_join_key(column.name(), original_left, original_right)
                {
                    columns.push(key);
                }
            } else {
                columns.push(column);
            }
        }
        columns.extend(
            right
                .into_iter()
                .filter(|column| !merged.contains(column.name())),
        );
        columns
    }
}

impl JoinExprDataSource {
    pub(super) fn projection_column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, crate::sem::AnalysisError> {
        let columns = self.project(context, with, None)?;
        Ok(super::projection::ProjectionColumns::from(columns).names())
    }
}
