use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, DataSource, MergeMatchKind, Nullable, WithClause,
        data_source::{
            ColumnCandidates, MergeRecordRole, ProjectionColumn, QualifiedSource, SourceNamespace,
            WildcardProjection,
        },
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// semantic representation for `FROM` clause
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FromClause(Vec<DataSource>);

#[derive(Clone, Copy)]
struct MergeScopeConfig {
    include_target: bool,
    include_source: bool,
    target_nullable: bool,
    source_nullable: bool,
    row_images: Option<MergeRowImageNullability>,
    merge_action_available: bool,
}

#[derive(Clone, Copy)]
struct MergeRowImageNullability {
    old: bool,
    new: bool,
}

impl FromClause {
    pub(crate) fn wildcard_projection(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        if let Some(qualifier) = qualifier {
            return match self.namespace().resolve_name(qualifier) {
                Ok(Some(source)) => source.resolved_wildcard_projection(context, with),
                Ok(None) => Ok(WildcardProjection::unresolved()),
                Err(()) => WildcardProjection::ambiguous_alias(context, qualifier),
            };
        }
        let mut columns = Vec::new();
        let mut table_dependent = false;
        for source in &self.0 {
            columns.extend(source.projection_names(context, with, None)?);
            table_dependent |= source.projection_is_table_dependent(context);
        }
        let source_found = !columns.is_empty()
            || (!self.0.is_empty() && self.0.iter().all(DataSource::projection_shape_is_resolved));
        Ok(WildcardProjection::new(
            &columns,
            source_found,
            table_dependent,
        ))
    }

    pub(crate) fn destination_wildcard_projection(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        if let Some(source) = self.0.last() {
            Ok(WildcardProjection::new(
                &source.projection_names(context, with, None)?,
                source.projection_shape_is_resolved(),
                source.projection_is_table_dependent(context),
            ))
        } else {
            Ok(WildcardProjection::new(&[], false, false))
        }
    }

    pub(crate) fn joined_projection(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<Option<ProjectionColumn<'_>>, AnalysisError> {
        if !self.0.iter().any(DataSource::is_joined) {
            return Ok(None);
        }
        let mut selected = None;
        for source in &self.0 {
            if source
                .aliases_for_column_resolved(column, context, with)
                .is_unknown()
            {
                return Ok(None);
            }
            for candidate in source.projection_names(context, with, None)? {
                if candidate.name() != column {
                    continue;
                }
                if selected.is_some() {
                    return Ok(None);
                }
                selected = Some(candidate);
            }
        }
        Ok(selected)
    }

    pub(crate) fn namespace(&self) -> SourceNamespace<'_> {
        let mut namespace = SourceNamespace::default();
        for source in &self.0 {
            namespace.append(source.namespace());
        }
        namespace
    }

    pub(crate) fn contains_alias(&self, name: &AliasName) -> bool {
        self.namespace().contains_alias(name)
    }

    pub(crate) fn destination_source(&self, alias: &AliasName) -> Option<QualifiedSource<'_>> {
        self.0
            .last()?
            .namespace()
            .resolve_name(alias)
            .ok()
            .flatten()
    }

    pub(crate) fn validate_source(
        &self,
        context: &mut impl ParseContext,
        source: &DataSource,
    ) -> Result<(), AnalysisError> {
        source
            .namespace()
            .validate_against(&self.namespace(), context)
    }

    pub(crate) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<QualifiedSource<'a>>, ()> {
        let mut result = None;
        for source in &self.0 {
            if let Some(candidate) = source.qualified_source(context, name)? {
                if result.is_some() {
                    return Err(());
                }
                result = Some(candidate);
            }
        }
        Ok(result)
    }

    pub(crate) fn for_merge_on(target: &DataSource, source: &DataSource) -> Self {
        Self::merge_scope(target, source, MergeScopeConfig::on())
    }

    pub(crate) fn for_merge_when(
        target: &DataSource,
        source: &DataSource,
        kind: MergeMatchKind,
    ) -> Self {
        Self::merge_scope(target, source, MergeScopeConfig::when(kind))
    }

    pub(crate) fn for_merge_returning(
        target: &DataSource,
        source: &DataSource,
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        Self::merge_scope(
            target,
            source,
            MergeScopeConfig::returning(
                source_nullable,
                old_nullable,
                new_nullable,
                row_images_enabled,
            ),
        )
    }
}

#[cfg(test)]
mod test_source_nullability_is_retained_across_postgres_versions {
    use crate::{
        names::ColumnName,
        sem::MergeRecordRole,
        test_helpers::{
            merge_assignments::strict_catalog, statements::resolve_postgres as resolve,
        },
    };

    #[rstest::rstest]
    #[case::postgres17(17)]
    #[case::postgres18(18)]
    fn source_nullability_is_retained_across_postgres_versions(#[case] version: u32) {
        let mut context = strict_catalog(version);
        let statement = resolve(
            "MERGE INTO strict_items t USING (SELECT 1 AS sid) s ON t.id=s.sid WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED BY SOURCE THEN DELETE RETURNING s.sid,t.id",
            &mut context,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let columns = statement.as_merge().unwrap().result_columns();
        let source = columns.get_column_at(0).unwrap();
        let target = columns.get_column_at(1).unwrap();
        assert_eq!(source.is_not_null(), &Some(false));
        assert_eq!(target.is_not_null(), &Some(true));
        assert_eq!(
            source
                .expression()
                .unwrap()
                .merge_record_reference()
                .unwrap()
                .role(),
            MergeRecordRole::source()
        );
        assert_eq!(
            target
                .expression()
                .unwrap()
                .merge_record_reference()
                .unwrap()
                .role(),
            MergeRecordRole::target()
        );
    }

    #[rstest::rstest]
    #[case::update_only("WHEN MATCHED THEN UPDATE SET id=s.sid", Some(true), Some(true))]
    #[case::insert_possible(
        "WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)",
        Some(false),
        Some(true)
    )]
    #[case::delete_possible("WHEN MATCHED THEN DELETE", Some(true), Some(false))]
    fn postgres18_row_images_keep_action_nullability(
        #[case] action: &str,
        #[case] old_not_null: Option<bool>,
        #[case] new_not_null: Option<bool>,
    ) {
        let mut context = strict_catalog(18);
        let sql = format!(
            "MERGE INTO strict_items t USING (SELECT 1 AS sid) s ON t.id=s.sid {action} RETURNING old.id,new.id"
        );
        let statement = resolve(&sql, &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let columns = statement.as_merge().unwrap().result_columns();
        let old = columns.get_column_at(0).unwrap();
        let new = columns.get_column_at(1).unwrap();
        assert_eq!(old.is_not_null(), &old_not_null);
        assert_eq!(new.is_not_null(), &new_not_null);
        assert_eq!(
            old.expression()
                .unwrap()
                .merge_record_reference()
                .unwrap()
                .role(),
            MergeRecordRole::old_row()
        );
        assert_eq!(
            new.expression()
                .unwrap()
                .merge_record_reference()
                .unwrap()
                .role(),
            MergeRecordRole::new_row()
        );
        assert_eq!(
            old.expression()
                .unwrap()
                .merge_record_reference()
                .unwrap()
                .source()
                .get_column_name(),
            &ColumnName::from("id")
        );
    }
}

impl FromClause {
    fn merge_scope(target: &DataSource, source: &DataSource, config: MergeScopeConfig) -> Self {
        let mut sources = Vec::new();
        if config.include_source {
            sources.push(DataSource::provided_merge_record(
                source,
                MergeRecordRole::source(),
                config.source_nullable,
                config.merge_action_available,
                false,
                false,
                false,
            ));
        }
        if config.include_target {
            let row_images = config.row_images.unwrap_or(MergeRowImageNullability {
                old: false,
                new: false,
            });
            sources.push(DataSource::provided_merge_record(
                target,
                MergeRecordRole::target(),
                config.target_nullable,
                config.merge_action_available,
                row_images.old,
                row_images.new,
                config.row_images.is_some(),
            ));
        }
        Self(sources)
    }

    pub(crate) fn merge_action_available(&self) -> bool {
        self.0.iter().any(DataSource::merge_action_available)
    }

    pub(crate) fn merge_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, MergeRecordRole, bool)> {
        if self.contains_alias(alias) {
            return None;
        }
        self.0
            .iter()
            .find_map(|source| source.merge_row_target(alias))
    }

    pub(crate) fn single_relation_name(&self) -> Option<FullName> {
        match self.0.as_slice() {
            [DataSource::Table(table)] => Some(table.name().full_name().clone()),
            [DataSource::View(view)] => Some(view.view_name().full_name().clone()),
            _ => None,
        }
    }
    /// add data source
    pub fn push(&mut self, source: DataSource) {
        self.0.push(source);
    }

    /// Resolve a unique visible alias. Missing or ambiguous aliases return `None`.
    #[must_use]
    pub fn resolve_alias(&self, alias_name: &AliasName) -> Option<(&DataSource, Nullable)> {
        self.namespace().resolve_name(alias_name).ok().flatten()?;
        for source in &self.0 {
            if let Some(resolved) = source.resolve_alias(alias_name) {
                return Some(resolved);
            }
        }
        None
    }

    /// Resolve the unique local alias supplying a column.
    ///
    /// `Unknown` means an unresolved source could supply the column; only
    /// `NotFound` permits enclosing-scope lookup. Competing known aliases are
    /// returned as an ambiguity error.
    pub fn find_alias_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> Result<::sql_fun_core::LookupResult<AliasSpec>, Vec<AliasSpec>>
    where
        TParseContext: ParseContext,
    {
        self.find_alias_resolved(column, context, with_clause)
    }
}

#[cfg(test)]
mod test_find_alias_for_column {
    use crate::{
        names::{AliasName, ColumnName},
        sem::{AliasSpec, WithClause},
        test_helpers::TestParseContext,
    };
    use ::sql_fun_core::LookupResult;

    #[rstest::rstest]
    #[case::found(
        "(SELECT 1 AS id) a",
        LookupResult::found(AliasSpec::from_names("a", &[]))
    )]
    #[case::absent("(SELECT 1 AS other) a", LookupResult::not_found())]
    #[case::unknown("missing.items a", LookupResult::unknown())]
    #[case::unknown_function("missing() a", LookupResult::unknown())]
    #[case::competing("(SELECT 1 AS id) a, missing.items b", LookupResult::unknown())]
    #[case::reversed("missing.items b, (SELECT 1 AS id) a", LookupResult::unknown())]
    fn preserves_lookup_conclusiveness(
        #[case] from_sql: &str,
        #[case] expected: LookupResult<AliasSpec>,
    ) -> testresult::TestResult {
        let (from, context) = TestParseContext::scalar_scope_from_sql(from_sql)?;
        let actual =
            from.find_alias_for_column(&ColumnName::from("id"), &context, &WithClause::default());
        assert_eq!(actual, Ok(expected));
        Ok(())
    }

    #[rstest::rstest]
    #[case::known("(SELECT 1 AS id) a")]
    #[case::absent("(SELECT 1 AS other) a")]
    #[case::unknown("missing.items a")]
    #[case::unknown_function("missing() a")]
    fn source_candidate_api_preserves_lookup_conclusiveness(
        #[case] from_sql: &str,
    ) -> testresult::TestResult {
        let (from, context) = TestParseContext::scalar_scope_from_sql(from_sql)?;
        let with = WithClause::default();
        let column = ColumnName::from("id");
        let (source, _) = from.resolve_alias(&AliasName::from("a")).unwrap();
        let expected = match from
            .find_alias_for_column(&column, &context, &with)
            .unwrap()
        {
            LookupResult::Found(alias) => LookupResult::found(vec![alias]),
            LookupResult::NotFound => LookupResult::not_found(),
            LookupResult::Unknown => LookupResult::unknown(),
        };
        assert_eq!(
            source.get_aliases_for_column(&column, &context, &with),
            expected
        );
        Ok(())
    }
}

impl FromClause {
    pub(crate) fn find_alias_resolved<TContext: ParseContext>(
        &self,
        column: &ColumnName,
        context: &TContext,
        with_clause: &WithClause,
    ) -> Result<::sql_fun_core::LookupResult<AliasSpec>, Vec<AliasSpec>> {
        let mut candidates = ColumnCandidates::default();
        for source in &self.0 {
            candidates.append(source.aliases_for_column_resolved(column, context, with_clause));
        }
        candidates.resolve()
    }
}

impl MergeScopeConfig {
    const fn on() -> Self {
        Self {
            include_target: true,
            include_source: true,
            target_nullable: false,
            source_nullable: false,
            row_images: None,
            merge_action_available: false,
        }
    }

    const fn when(kind: MergeMatchKind) -> Self {
        Self {
            include_target: kind.has_target(),
            include_source: kind.has_source(),
            target_nullable: false,
            source_nullable: false,
            row_images: None,
            merge_action_available: false,
        }
    }

    const fn returning(
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
        row_images_enabled: bool,
    ) -> Self {
        Self {
            include_target: true,
            include_source: true,
            target_nullable: false,
            source_nullable,
            row_images: if row_images_enabled {
                Some(MergeRowImageNullability {
                    old: old_nullable,
                    new: new_nullable,
                })
            } else {
                None
            },
            merge_action_available: true,
        }
    }
}

impl FromClause {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for source in &mut self.0 {
            source.walk(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for source in &self.0 {
            source.visit(visitor)?;
        }
        Ok(())
    }
}
