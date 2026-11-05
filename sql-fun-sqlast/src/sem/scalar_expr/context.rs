use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{AliasName, ColumnName, FullName, SchemaName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnCollection, ColumnDefinition, ColumnReferenceExpr,
        CreateTable, DataSource, DeleteRowVersion, FromClause, MergeMatchKind, QueryNameResolution,
        QueryProblem, ResultColumnCollection, RuleEvent, RuleIdentity, SemScalarExpr, WholeRowExpr,
        WholeRowFields, WithClause,
        data_source::{MergeRecordRole, QualifiedSource, WildcardProjection},
        problem::{RuleProblem, TriggerProblem},
        scalar_expr::{
            column_ref::{QualifiedColumnFailure, UndefinedAliasColumnRef},
            grouping::GroupingScope,
        },
    },
};

use std::borrow::Cow;

use crate::sem::scalar_expr::grouping::ReferenceCapture;

/// Expression-local scope used during scalar semantic analysis.
///
/// This is separate from [`ParseContext`], which provides schema
/// information and collects diagnostics. Nested scalar expressions share this
/// scope, so additional contextual bindings can be introduced here without
/// changing every expression analyzer's arguments.
#[derive(Debug, Default)]
pub struct ScalarAnalysisContext<'a> {
    with_clause: Cow<'a, WithClause>,
    from_clause: Cow<'a, FromClause>,
    parent: Option<Box<Self>>,
    windows: Vec<String>,
    grouping: GroupingScope,
    output_aliases: Option<ResultColumnCollection>,
    delete_returning_target: Option<AliasName>,
    postgres_returning: bool,
    qualified_only: bool,
    rule: Option<RuleScope>,
    trigger_namespace: Option<SchemaName>,
    projection_scope_id: Option<usize>,
    declared_table: Option<DeclaredTableRow<'a>>,
}

/// Row declarations can precede catalog publication, but bindings use their canonical identity.
#[derive(Debug, Clone)]
struct DeclaredTableRow<'a> {
    definition: Cow<'a, ColumnCollection>,
    identity: TableName,
}

#[derive(Debug, Clone)]
struct RuleScope {
    identity: RuleIdentity,
    event: RuleEvent,
    forbidden: Option<&'static str>,
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn declared_table(table: &'a CreateTable, identity: &TableName) -> Self {
        Self::declared_columns(
            Cow::Owned(table.declared_columns().check_scope_columns()),
            identity,
        )
    }

    pub(crate) fn declared_columns(
        columns: Cow<'a, ColumnCollection>,
        identity: &TableName,
    ) -> Self {
        Self {
            declared_table: Some(DeclaredTableRow {
                definition: columns,
                identity: identity.clone(),
            }),
            ..Self::default()
        }
    }

    pub(crate) fn column_declaration(
        context: &impl ParseContext,
        identity: &TableName,
        column: &ColumnDefinition,
    ) -> Self {
        let columns = context
            .get_table(identity)
            .map(|table| table.declared_columns().check_scope_columns())
            .unwrap_or_default()
            .with_declared_column(column);
        Self::declared_columns(Cow::Owned(columns), identity)
    }

    fn resolve_declared_column(
        &self,
        context: &mut impl ParseContext,
        alias: Option<&AliasName>,
        column: &ColumnName,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if let Some(table) = &self.declared_table
            && alias.is_none_or(|alias| {
                if context.dialect() == Dialect::sqlite() {
                    alias
                        .as_str()
                        .eq_ignore_ascii_case(table.identity.full_name().name().as_str())
                } else {
                    alias.as_str() == table.identity.full_name().name().as_str()
                }
            })
        {
            if alias.is_none()
                && context.dialect() == Dialect::postgres()
                && column.as_str() == table.identity.full_name().name().as_str()
                && table.definition.find_column(column, false).is_none()
            {
                return self
                    .declared_whole_row(context, table.identity.full_name(), 0)
                    .map(Some);
            }
            return self.declared_column(context, column).map(Some);
        }
        Ok(None)
    }

    fn declared_column(
        &self,
        context: &mut impl ParseContext,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let table = self.declared_table.as_ref().expect("declared row scope");
        let definition = table
            .definition
            .find_column(column, context.dialect() == Dialect::sqlite());
        let canonical = if let Some(binding) = definition
            && let Some(name) = binding.name().as_ref()
        {
            name
        } else {
            column
        };
        if definition.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                &table.identity,
                column,
            ))?;
        }
        Ok(SemScalarExpr::ColumnRef(ColumnReferenceExpr::table_column(
            &AliasName::from(table.identity.full_name().name().as_str()),
            canonical,
            &table.identity,
            definition,
            false,
        )))
    }
}

impl ScalarAnalysisContext<'_> {
    fn declared_whole_row(
        &self,
        context: &mut impl ParseContext,
        requested: &FullName,
        levels_up: usize,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let table = self.declared_table.as_ref().expect("declared row scope");
        let mut fields = WholeRowFields::default();
        for column in table.definition.names() {
            fields.push(&column, &self.declared_column(context, &column)?);
        }
        let alias = AliasName::from(table.identity.full_name().name().as_str());
        Ok(SemScalarExpr::whole_row(&WholeRowExpr::resolved(
            requested,
            &alias,
            Some(table.identity.full_name()),
            None,
            levels_up,
            Some(true),
            &fields,
        )))
    }
    fn notify_unbound_row_image(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
    ) -> Result<bool, AnalysisError> {
        if self.postgres_returning
            && matches!(alias.as_str(), "old" | "new")
            && !self.in_rule_definition()
        {
            use crate::engine_specific::{Postgres18Feature, Postgres18Syntax};
            if context.postgres18_syntax_used(Postgres18Syntax::ReturningRowImages)? {
                todo!("RETURNING destination row-image expressions (#621/#614)");
            }
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod unimplemented_returning_image_tests {
    use crate::{
        names::AliasName,
        sem::{FromClause, WithClause, scalar_expr::context::ScalarAnalysisContext},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[should_panic(expected = "RETURNING destination row-image expressions")]
    fn unbound_images_reach_todo(#[values("old", "new")] name: &str) {
        let mut context = TestParseContext::default().with_postgres_version("18");
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut scope = ScalarAnalysisContext::new(&with_clause, &from_clause);
        scope.set_postgres_returning(&context);
        let _ = scope.notify_unbound_row_image(&mut context, &AliasName::from(name));
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) const fn grouping_scope(&self) -> &GroupingScope {
        &self.grouping
    }
    pub(crate) const fn set_grouping_query(&mut self, id: usize) {
        self.grouping.set_query(id);
        self.projection_scope_id = Some(id);
    }

    pub(crate) fn with_grouping_capture(&self, capture: &ReferenceCapture) -> Self {
        let mut scope = self.clone_scope();
        scope.add_grouping_capture(capture);
        scope
    }
    fn add_grouping_capture(&mut self, capture: &ReferenceCapture) {
        self.grouping.add_capture(capture);
        if let Some(parent) = &mut self.parent {
            parent.add_grouping_capture(capture);
        }
    }
    pub(crate) fn with_grouping_placement(&self, placement: &str) -> Self {
        let mut scope = self.clone_scope();
        scope.grouping.set_placement(placement);
        scope
    }

    /// Replace visible sources without introducing another SQL query level.
    pub(crate) fn with_from_clause(&self, from: &FromClause) -> Self {
        let mut scope = self.clone_scope();
        scope.from_clause = Cow::Owned(from.clone());
        scope
    }

    pub(crate) fn with_added_source(&self, source: &DataSource) -> Self {
        let mut scope = self.clone_scope();
        scope.from_clause.to_mut().push(source.clone());
        scope
    }
    pub(crate) fn rule_action_source(
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&Self>,
    ) -> Self {
        let mut scope = Self::nested(with, from, outer);
        scope.rule = if let Some(parent) = outer {
            parent.rule.clone()
        } else {
            None
        };
        scope
    }
    pub(crate) fn insert_returning_outer(&self, context: &impl ParseContext) -> Option<Self> {
        if self.in_rule_definition() && postgres_18_or_later(context) {
            let mut restricted = self.clone_scope();
            restricted.forbid_rule_rows(
                "PostgreSQL 18 INSERT RETURNING destination row images are pending #621",
            );
            Some(restricted)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_insert_returning_does_not_bind_enclosing_rule_rows {
    use crate::test_helpers::{rule as test_helpers, statements};

    #[rstest::rstest]
    fn insert_returning_does_not_bind_enclosing_rule_rows(
        #[values("15", "16", "17", "18")] version: &str,
    ) {
        let mut context = test_helpers::catalog_version(version);
        statements::resolve_postgres(
            "CREATE RULE r AS ON UPDATE TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING OLD.id",
            &mut context,
        ).unwrap();
        if version == "18" {
            let mut has_boundary = false;
            for problem in context.reported_problems() {
                if let Some(rule) = problem.rule_problem() {
                    has_boundary |= rule
                        .definition_detail()
                        .is_some_and(|detail| detail.contains("#621"));
                }
            }
            assert!(has_boundary);
        }
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn with_clause(&self) -> &WithClause {
        &self.with_clause
    }
    pub(crate) fn projection_names(
        &self,
        context: &mut impl ParseContext,
        qualifier: &AliasName,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        match self.from_clause.namespace().resolve_name(qualifier) {
            Ok(Some(source)) => source.resolved_wildcard_projection(context, &self.with_clause),
            Err(()) => WildcardProjection::ambiguous_alias(context, qualifier),
            Ok(None) => self.parent_projection_names(context, qualifier),
        }
    }

    fn parent_projection_names(
        &self,
        context: &mut impl ParseContext,
        qualifier: &AliasName,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        let Some(parent) = &self.parent else {
            return Ok(WildcardProjection::unresolved());
        };
        let mut projection = parent.projection_names(context, qualifier)?;
        projection.cross_parent_scope(self.projection_scope_id != parent.projection_scope_id);
        Ok(projection)
    }

    pub(crate) fn relation_projection_names(
        &self,
        context: &mut impl ParseContext,
        relation: &FullName,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        match self.from_clause.qualified_source(context, relation) {
            Ok(Some(source)) => source.resolved_wildcard_projection(context, &self.with_clause),
            Err(()) => self.ambiguous_relation_projection(context),
            Ok(None) => self.parent_relation_projection_names(context, relation),
        }
    }

    fn parent_relation_projection_names(
        &self,
        context: &mut impl ParseContext,
        relation: &FullName,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        let Some(parent) = &self.parent else {
            return Ok(WildcardProjection::unresolved());
        };
        let mut projection = parent.relation_projection_names(context, relation)?;
        projection.cross_parent_scope(self.projection_scope_id != parent.projection_scope_id);
        Ok(projection)
    }

    fn ambiguous_relation_projection(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<WildcardProjection<'_>, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query(
            QueryProblem::ambiguous_reference("star source"),
        ))?;
        Ok(WildcardProjection::new(&[], true, false))
    }

    pub(crate) fn rule(
        with: &'a WithClause,
        from: &'a FromClause,
        identity: &RuleIdentity,
        event: RuleEvent,
        predicate: bool,
    ) -> Self {
        let mut scope = Self::new(with, from);
        scope.qualified_only = !predicate;
        scope.rule = Some(RuleScope {
            identity: identity.clone(),
            event,
            forbidden: None,
        });
        scope
    }
    pub(crate) fn validate_rule_sources(
        &self,
        context: &mut impl ParseContext,
        from: &FromClause,
    ) -> Result<(), AnalysisError> {
        if let Some(identity) = self.direct_rule_identity() {
            for name in ["old", "new"] {
                if from.contains_alias(&AliasName::from(name)) {
                    context.report_analysis_problem(AnalysisProblem::rule(
                        &RuleProblem::definition(
                            identity,
                            "action source conflicts with rule OLD/NEW namespace",
                        ),
                    ))?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn without_rule_rows(&self) -> Self {
        let mut scope = self.clone_scope();
        scope.forbid_rule_rows("OLD/NEW are not allowed in this rule subquery");
        scope
    }
    fn forbid_rule_rows(&mut self, detail: &'static str) {
        if let Some(rule) = &mut self.rule {
            rule.forbidden = Some(detail);
        }
        if let Some(parent) = &mut self.parent {
            parent.forbid_rule_rows(detail);
        }
    }
    pub(crate) fn in_rule_definition(&self) -> bool {
        self.rule.is_some()
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.in_rule_definition())
    }
    pub(crate) fn rule_identity(&self) -> Option<&RuleIdentity> {
        if let Some(rule) = &self.rule {
            Some(&rule.identity)
        } else if let Some(parent) = &self.parent {
            parent.rule_identity()
        } else {
            None
        }
    }
    pub(crate) fn direct_rule_identity(&self) -> Option<&RuleIdentity> {
        self.rule
            .as_ref()
            .filter(|rule| rule.forbidden.is_none())
            .map(|rule| &rule.identity)
    }
    fn validate_rule_reference(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
    ) -> Result<(), AnalysisError> {
        if let Some(rule) = &self.rule
            && (rule.forbidden.is_some() || !rule.event.permits_row(alias.as_str()))
        {
            let detail = rule
                .forbidden
                .unwrap_or("OLD/NEW reference is not available for this event");
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                &rule.identity,
                detail,
            )))?;
        }
        Ok(())
    }

    pub(crate) const fn trigger(with: &'a WithClause, from: &'a FromClause) -> Self {
        let mut scope = Self::new(with, from);
        scope.qualified_only = true;
        scope
    }
    pub(crate) fn set_trigger_namespace(&mut self, schema: Option<&SchemaName>) {
        self.trigger_namespace = schema.cloned();
    }
    fn trigger_namespace(&self) -> Option<&SchemaName> {
        if let Some(schema) = &self.trigger_namespace {
            Some(schema)
        } else if let Some(parent) = &self.parent {
            parent.trigger_namespace()
        } else {
            None
        }
    }
    pub(crate) fn trigger_relation_name(
        &self,
        context: &mut impl ParseContext,
        name: &FullName,
    ) -> Result<FullName, AnalysisError> {
        if context.dialect() != Dialect::sqlite() {
            return Ok(name.clone());
        }
        let Some(schema) = self.trigger_namespace() else {
            return Ok(name.clone());
        };
        if name.catalog().is_some()
            || name
                .schema()
                .is_some_and(|supplied| !supplied.as_str().eq_ignore_ascii_case(schema.as_str()))
        {
            context.report_analysis_problem(AnalysisProblem::trigger(&TriggerProblem::options(
                name,
                "non-TEMP SQLite triggers can reference only their own database",
            )))?;
        }
        Ok(name.replace_schema(schema))
    }
}

#[cfg(test)]
mod test_sqlite_body_uses_trigger_database {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, names::FullName, test_helpers::trigger as test_helpers};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::read("CREATE TRIGGER aux.audit AFTER UPDATE ON items BEGIN SELECT id FROM other; END")]
    #[case::write(
        "CREATE TRIGGER aux.audit AFTER UPDATE ON items BEGIN UPDATE OTHER SET id = NEW.id; END"
    )]
    fn sqlite_body_uses_trigger_database(#[case] sql: &str) -> testresult::TestResult {
        let oracle = test_helpers::sqlite_oracle()?;
        oracle.execute_batch("ATTACH ':memory:' AS aux; CREATE TABLE aux.items(id INTEGER); CREATE TABLE aux.other(id INTEGER);")?;
        oracle.execute_batch(sql)?;
        let mut context = test_helpers::catalog(Dialect::sqlite());
        test_helpers::add_tables(&mut context, "aux");
        let statement = test_helpers::resolve(sql, &mut context)?;
        let trigger = statement.trigger().unwrap();
        assert_eq!(trigger.target(), &FullName::with_schema("aux", "items"));
        assert!(
            trigger
                .dependencies()
                .relations()
                .contains(&FullName::with_schema("aux", "other"))
        );
        assert!(
            !trigger
                .dependencies()
                .relations()
                .contains(&FullName::with_schema("main", "other"))
        );
        Ok(())
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn trigger_scope(&self) -> Option<&Self> {
        if self.qualified_only {
            Some(self)
        } else if let Some(parent) = &self.parent {
            parent.trigger_scope()
        } else {
            None
        }
    }
    fn clone_scope(&self) -> Self {
        Self {
            declared_table: self.declared_table.clone(),
            windows: self.windows.clone(),
            grouping: self.grouping.clone(),
            output_aliases: self.output_aliases.clone(),
            delete_returning_target: self.delete_returning_target.clone(),
            postgres_returning: self.postgres_returning,
            qualified_only: self.qualified_only,
            rule: self.rule.clone(),
            trigger_namespace: self.trigger_namespace.clone(),
            projection_scope_id: self.projection_scope_id,
            with_clause: self.with_clause.clone(),
            from_clause: self.from_clause.clone(),
            parent: self
                .parent
                .as_deref()
                .map(|parent| Box::new(parent.clone_scope())),
        }
    }

    /// Borrow the CTE and data-source scopes visible to an expression.
    pub const fn new(with_clause: &'a WithClause, from_clause: &'a FromClause) -> Self {
        Self {
            with_clause: Cow::Borrowed(with_clause),
            from_clause: Cow::Borrowed(from_clause),
            parent: None,
            declared_table: None,
            windows: Vec::new(),
            grouping: GroupingScope::new(),
            output_aliases: None,
            delete_returning_target: None,
            postgres_returning: false,
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
            projection_scope_id: None,
        }
    }

    pub(crate) fn nested(
        with_clause: &'a WithClause,
        from_clause: &'a FromClause,
        parent: Option<&Self>,
    ) -> Self {
        Self {
            with_clause: Cow::Borrowed(with_clause),
            from_clause: Cow::Borrowed(from_clause),
            parent: parent.map(|parent| Box::new(parent.clone_scope())),
            declared_table: None,
            windows: Vec::new(),
            grouping: GroupingScope::nested(parent.map(|scope| &scope.grouping)),
            output_aliases: None,
            delete_returning_target: None,
            postgres_returning: false,
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
            projection_scope_id: if let Some(parent) = parent {
                parent.projection_scope_id
            } else {
                None
            },
        }
    }

    pub(crate) fn for_merge_on(
        with_clause: &'a WithClause,
        target: &DataSource,
        source: &DataSource,
    ) -> Self {
        Self::with_owned_from_clause(with_clause, FromClause::for_merge_on(target, source))
    }

    pub(crate) fn for_merge_when(
        with_clause: &'a WithClause,
        target: &DataSource,
        source: &DataSource,
        kind: MergeMatchKind,
    ) -> Self {
        Self::with_owned_from_clause(
            with_clause,
            FromClause::for_merge_when(target, source, kind),
        )
    }

    pub(crate) fn for_merge_returning(
        context: &impl ParseContext,
        with_clause: &'a WithClause,
        target: &DataSource,
        source: &DataSource,
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
    ) -> Self {
        let mut scope = Self::with_owned_from_clause(
            with_clause,
            FromClause::for_merge_returning(
                target,
                source,
                source_nullable,
                old_nullable,
                new_nullable,
                postgres_18_or_later(context),
            ),
        );
        scope.set_postgres_returning(context);
        scope
    }

    pub(crate) fn sources(&self) -> &FromClause {
        self.from_clause.as_ref()
    }

    fn rule_rows_visible(&self) -> bool {
        if let Some(rule) = &self.rule {
            rule.forbidden.is_none()
        } else if let Some(parent) = &self.parent {
            parent.rule_rows_visible()
        } else {
            false
        }
    }
    pub(crate) fn set_postgres_returning(&mut self, context: &impl ParseContext) {
        self.postgres_returning = context.dialect() == Dialect::postgres();
    }

    pub(crate) fn check_returning_star(
        &self,
        context: &mut impl ParseContext,
        alias: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let Some(alias) = alias else {
            return Ok(false);
        };
        if self.delete_row_target(alias).is_some() || self.merge_row_target(alias).is_some() {
            use crate::engine_specific::{Postgres18Feature, Postgres18Syntax};
            context.postgres18_syntax_used(Postgres18Syntax::ReturningRowImages)?;
            return Ok(false);
        }
        if self
            .from_clause
            .namespace()
            .resolve_name(alias)
            .is_ok_and(|source| source.is_none())
        {
            return self.notify_unbound_row_image(context, alias);
        }
        Ok(false)
    }

    pub(crate) fn set_delete_returning_target(
        &mut self,
        target: &AliasName,
        context: &impl ParseContext,
    ) {
        if self.rule_rows_visible() {
            return;
        }
        if context.dialect() != Dialect::postgres() {
            return;
        }
        self.postgres_returning = true;
        self.delete_returning_target = Some(target.clone());
    }
}

#[cfg(test)]
mod test_cte_delete_returning_uses_its_destination_row_image_on_postgres18 {
    use crate::{
        names::FullName,
        test_helpers::{rule as test_helpers, statements},
    };

    #[test]
    fn cte_delete_returning_uses_its_destination_row_image_on_postgres18() -> testresult::TestResult
    {
        let mut context = test_helpers::catalog_version("18");
        test_helpers::add_table_column(
            &mut context,
            &FullName::with_schema("public", "sink"),
            "other",
        );
        statements::resolve_postgres(
            "CREATE RULE r AS ON UPDATE TO items DO WITH x AS (DELETE FROM sink RETURNING OLD.other) SELECT other FROM x",
            &mut context,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod test_returning_row_images_use_configured_engine_version {
    use crate::{
        ast::Dialect,
        test_helpers::{delete::resolve, delete_versions as test_helpers},
    };

    #[rstest::rstest]
    #[case::postgres17("17", true)]
    #[case::postgres18("18", false)]
    #[case::custom("custom-engine", true)]
    fn returning_row_images_use_configured_engine_version(
        #[case] version: &str,
        #[case] missing: bool,
    ) {
        let mut context = test_helpers::catalog_for_version(version);
        let statement = resolve(
            "DELETE FROM items RETURNING old.id,new.id",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(context.reported_problem_count() > 0, missing);
        if !missing {
            let columns = statement.as_delete().unwrap().result_columns();
            assert_eq!(columns.get_column_at(0).unwrap().is_not_null(), &Some(true));
            assert_eq!(
                columns.get_column_at(1).unwrap().is_not_null(),
                &Some(false)
            );
        }
    }

    #[rstest::rstest]
    fn explicit_old_alias_is_ordinary_source(#[values("17", "18")] version: &str) {
        let mut context = test_helpers::catalog_for_version(version);
        let result = resolve(
            "DELETE FROM items AS old RETURNING old.id",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert!(
            result
                .as_delete()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .delete_row_reference()
                .is_none()
        );
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn delete_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, DeleteRowVersion)> {
        if self.from_clause.contains_alias(alias) {
            return None;
        }
        let version = DeleteRowVersion::from_alias(alias.as_str())?;
        Some((self.delete_returning_target.as_ref()?, version))
    }
    pub(crate) fn set_output_aliases(&mut self, columns: &ResultColumnCollection) {
        self.output_aliases = Some(columns.clone());
    }
    pub(crate) fn has_input_column(
        &self,
        context: &impl ParseContext,
        column: &ColumnName,
    ) -> Result<bool, AnalysisError> {
        let lookup =
            self.from_clause
                .namespace()
                .resolve_column(context, &self.with_clause, column)?;
        if let Ok(::sql_fun_core::LookupResult::NotFound) = lookup {
            if let Some(parent) = &self.parent {
                parent.has_input_column(context, column)
            } else {
                Ok(false)
            }
        } else {
            Ok(true)
        }
    }
    pub(crate) fn resolve_lock(&self, name: &str) -> QueryNameResolution {
        let alias = AliasName::from(name);
        match self.from_clause.namespace().resolve_name(&alias) {
            Ok(Some(_)) => QueryNameResolution::source(&alias),
            Ok(None) => QueryNameResolution::not_found(),
            Err(()) => QueryNameResolution::ambiguous(),
        }
    }
}

#[cfg(test)]
mod test_locking_a_short_ambiguous_source_retains_failure {

    use crate::test_helpers::query::source_namespace_statement;

    #[test]
    fn locking_a_short_ambiguous_source_retains_failure() {
        let analyzed = source_namespace_statement("SELECT a.t.id FROM a.t,b.t FOR UPDATE OF t");
        assert!(
            analyzed
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "query_reference_ambiguous"),
            "{:?}",
            analyzed.context().reported_problems()
        );
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn with_window_names(&self, names: Vec<String>) -> Self {
        let mut scope = self.clone_scope();
        scope.windows = names;
        scope
    }
    pub(crate) fn set_windows(&mut self, names: Vec<String>) {
        self.windows = names;
    }
    pub(crate) fn resolve_window(
        &self,
        name: &str,
        context: &impl ParseContext,
    ) -> QueryNameResolution {
        if context.dialect() == Dialect::sqlite() {
            return self
                .windows
                .iter()
                .rposition(|candidate| candidate == name)
                .map_or_else(QueryNameResolution::not_found, QueryNameResolution::window);
        }
        let mut candidates = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.as_str() == name);
        let Some((index, _)) = candidates.next() else {
            return QueryNameResolution::not_found();
        };
        if candidates.next().is_some() {
            QueryNameResolution::ambiguous()
        } else {
            QueryNameResolution::window(index)
        }
    }
}

#[cfg(test)]
mod test_records_window_resolution {
    use crate::{
        ast::Dialect,
        sem::{AnalysisError, QueryNameResolution, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::found(
        "SELECT sum(1) OVER w WINDOW w AS ()",
        QueryNameResolution::WindowAlias(0),
        0
    )]
    #[case::missing("SELECT sum(1) OVER w", QueryNameResolution::NotFound, 1)]
    #[case::ambiguous(
        "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
        QueryNameResolution::Ambiguous,
        2
    )]
    fn records_window_resolution(
        #[case] sql: &str,
        #[case] expected: QueryNameResolution,
        #[case] problems: usize,
    ) {
        let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), problems);
        struct Windows(Vec<QueryNameResolution>);
        impl crate::sem::traversal::SemanticVisitorMut for Windows {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::FuncCall(call) = expression {
                    self.0.push(
                        call.modifiers()
                            .over()
                            .unwrap()
                            .reference()
                            .unwrap()
                            .resolution()
                            .clone(),
                    );
                    assert_matches::assert_matches!(
                        call.name_resolution(),
                        QueryNameResolution::Function(_)
                    );
                }
                Ok(())
            }
        }
        let mut windows = Windows(Vec::new());
        query.walk(&mut windows).unwrap();
        assert_eq!(windows.0, [expected]);
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) fn inherited_ctes(&self) -> WithClause {
        self.with_clause.as_ref().clone()
    }

    pub(crate) fn merge_action_available(&self) -> bool {
        self.from_clause.merge_action_available()
            || self
                .parent
                .as_deref()
                .is_some_and(Self::merge_action_available)
    }

    pub(crate) fn merge_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, MergeRecordRole, bool)> {
        self.from_clause.merge_row_target(alias)
    }

    const fn with_owned_from_clause(with_clause: &'a WithClause, from_clause: FromClause) -> Self {
        Self {
            with_clause: Cow::Borrowed(with_clause),
            from_clause: Cow::Owned(from_clause),
            parent: None,
            declared_table: None,
            windows: Vec::new(),
            grouping: GroupingScope::new(),
            output_aliases: None,
            delete_returning_target: None,
            postgres_returning: false,
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
            projection_scope_id: None,
        }
    }
}

fn postgres_18_or_later(context: &impl ParseContext) -> bool {
    if context.dialect() != Dialect::postgres() {
        return false;
    }
    let Some(version) = context.metadata().engine_version() else {
        return false;
    };
    version
        .to_string()
        .parse::<::sql_fun_core::PostgresVersion>()
        .is_ok_and(|version| version.major() >= 18)
}

impl ScalarAnalysisContext<'_> {
    pub(crate) fn resolve_qualified_column(
        &self,
        context: &mut impl ParseContext,
        qualifier: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        use crate::sem::scalar_expr::column_ref::QualifiedColumnFailure;
        let mut relation = qualifier.clone();
        if let Some(catalog) = qualifier.catalog() {
            if context.dialect() == Dialect::sqlite() {
                return QualifiedColumnFailure::invalid(context, qualifier, column);
            }
            let Some(database) = context.metadata().database_name() else {
                return QualifiedColumnFailure::database_required(context, qualifier, column);
            };
            if catalog.as_str() != database {
                return QualifiedColumnFailure::cross_database(context, qualifier, column);
            }
            relation = FullName::new(&None, &qualifier.schema().cloned(), qualifier.name());
        }
        self.resolve_qualified_in_scope(context, &relation, qualifier, column)
    }

    fn resolve_qualified_in_scope(
        &self,
        context: &mut impl ParseContext,
        relation: &FullName,
        requested: &FullName,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        use crate::sem::scalar_expr::column_ref::QualifiedColumnFailure;
        if let Some(table) = &self.declared_table {
            let name = table.identity.full_name();
            if (context.dialect() == Dialect::sqlite() && relation.sqlite_eq(name))
                || (context.dialect() != Dialect::sqlite() && relation == name)
            {
                return self.declared_column(context, column);
            }
        }
        // Row-image environments expose OLD/NEW aliases rather than relation names.
        let source = if self.qualified_only {
            Ok(None)
        } else {
            self.from_clause.qualified_source(context, relation)
        };
        match source {
            Ok(Some(source)) => {
                self.grouping.reference();
                source.bind(context, &self.with_clause, column)
            }
            Err(()) => QualifiedColumnFailure::ambiguous(context, requested, column),
            Ok(None) => {
                if let Some(parent) = &self.parent {
                    parent.resolve_qualified_in_scope(context, relation, requested, column)
                } else {
                    QualifiedColumnFailure::not_found(context, requested, column)
                }
            }
        }
    }
}

#[cfg(test)]
mod test_returning_distinguishes_rule_rows_from_action_destination {
    use crate::{
        names::FullName,
        test_helpers::{rule as test_helpers, statements},
    };

    #[rstest::rstest]
    #[case::update("UPDATE sink SET other=NEW.id RETURNING OLD.id", true)]
    #[case::delete("DELETE FROM sink RETURNING OLD.id", true)]
    #[case::update_wrong_relation("UPDATE sink SET other=NEW.id RETURNING OLD.other", false)]
    #[case::delete_wrong_relation("DELETE FROM sink RETURNING OLD.other", false)]
    fn returning_distinguishes_rule_rows_from_action_destination(
        #[values("15", "16", "17", "18")] version: &str,
        #[case] action: &str,
        #[case] accepted: bool,
    ) -> testresult::TestResult {
        let mut context = test_helpers::catalog_version(version);
        test_helpers::add_table_column(
            &mut context,
            &FullName::with_schema("public", "sink"),
            "other",
        );
        let result = statements::resolve_postgres(
            &format!("CREATE RULE r AS ON UPDATE TO items DO INSTEAD {action}"),
            &mut context,
        );
        let statement = result?;
        assert_eq!(
            statement.rule().is_some(),
            accepted,
            "{version}: {statement:?}"
        );
        if accepted {
            let reference = statement.rule().unwrap().actions()[0]
                .rule_returning()
                .unwrap()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap();
            assert_eq!(
                reference.relation(),
                &FullName::with_schema("public", "items")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_missing_local_column_does_not_bind_outer_source {

    use crate::{
        names::FullName,
        test_helpers::{query::source_namespace_statement, query_source_namespace as test_helpers},
    };

    #[rstest::rstest]
    #[case::qualified("SELECT (SELECT t.only_b FROM a.t) FROM b.t")]
    #[case::star("SELECT (SELECT t.only_b FROM a.t) FROM b.t AS t")]
    fn missing_local_column_does_not_bind_outer_source(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "column_not_found_in_table")
        );
        let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let mut bindings = test_helpers::TableBindings::default();
        query.walk(&mut bindings).unwrap();
        bindings.assert_only_relation(&FullName::with_schema("a", "t"));
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_none()
        );
    }

    #[test]
    fn an_unresolved_join_input_blocks_qualified_binding() {
        let analyzed = source_namespace_statement(
            "SELECT (SELECT j.id FROM (a.t CROSS JOIN missing.t) j) FROM b.t",
        );
        assert!(
            analyzed
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "relation_not_found")
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_none()
        );
    }
}

impl ScalarAnalysisContext<'_> {
    /// Resolve a column reference in this scope, reporting missing or ambiguous names.
    ///
    /// An alias restricts lookup to that data source. Without an alias, the column
    /// must identify exactly one source. Source nullability is preserved in the
    /// resulting expression.
    pub fn resolve_column_ref<TParseContext>(
        &self,
        mut context: TParseContext,
        alias: Option<&AliasName>,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (value, _) = self.resolve_column(&mut context, alias, column)?;
        Ok((value, context))
    }
}

#[cfg(test)]
mod test_resolve_column_ref {
    use crate::{
        names::{AliasName, ColumnName},
        sem::{
            ColumnDefinition, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr, SubQueryColumnRef,
            WithClause, scalar_expr::context::ScalarAnalysisContext,
        },
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case(None)]
    #[case(Some("missing"))]
    fn empty_scope_reports_missing_reference(
        #[case] alias: Option<&str>,
    ) -> testresult::TestResult {
        let column = ColumnName::from("n");
        let alias = alias.map(AliasName::from);
        let (expr, context) = ScalarAnalysisContext::default().resolve_column_ref(
            TestParseContext::default(),
            alias.as_ref(),
            &column,
        )?;
        if alias.is_some() {
            assert_matches::assert_matches!(
                expr,
                SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedAlias(_))
            );
        } else {
            assert_eq!(
                expr,
                SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(column))
            );
        }
        assert_eq!(expr.get_type(), None);
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }

    #[rstest::rstest]
    #[case("(select 42 as n) a", None)]
    #[case("(select 42 as n) a", Some("a"))]
    #[case("(select 42 as n) a, (select true as n) b", Some("a"))]
    fn resolves_column_and_preserves_metadata(
        #[case] from_sql: &str,
        #[case] alias: Option<&str>,
    ) -> testresult::TestResult {
        let (from, context) = TestParseContext::scalar_scope_from_sql(from_sql)?;
        let with = WithClause::default();
        let scope = ScalarAnalysisContext::new(&with, &from);
        let column = ColumnName::from("n");
        let alias = alias.map(AliasName::from);
        let (expr, context) = scope.resolve_column_ref(context, alias.as_ref(), &column)?;
        let definition = ColumnDefinition::new(
            &Some(column.clone()),
            Some(&PgBuiltInType::int4()),
            Some(true),
            &[],
        );
        assert_eq!(
            expr,
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::SubQueryColumn(SubQueryColumnRef::new(
                &AliasName::from("a"),
                &column,
                Some(&definition),
                false
            ),))
        );
        assert_eq!(expr.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(expr.is_not_null(), Some(true));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }

    #[test]
    fn unqualified_column_reports_ambiguity() -> testresult::TestResult {
        let (from, context) =
            TestParseContext::scalar_scope_from_sql("(select 42 as n) a, (select 43 as n) b")?;
        let with = WithClause::default();
        let scope = ScalarAnalysisContext::new(&with, &from);
        let column = ColumnName::from("n");
        let (expr, context) = scope.resolve_column_ref(context, None, &column)?;
        assert_eq!(
            expr,
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(column))
        );
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }

    #[test]
    fn qualified_lookup_does_not_fall_back_to_another_source() -> testresult::TestResult {
        let (from, context) =
            TestParseContext::scalar_scope_from_sql("(select 42 as n) a, (select 43 as m) b")?;
        let with = WithClause::default();
        let scope = ScalarAnalysisContext::new(&with, &from);
        let alias = AliasName::from("a");
        let column = ColumnName::from("m");
        let (expr, context) = scope.resolve_column_ref(context, Some(&alias), &column)?;
        assert_eq!(
            expr,
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::SubQueryColumn(SubQueryColumnRef::new(
                &alias, &column, None, false
            ),))
        );
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }

    #[rstest::rstest]
    #[case("left", "a", "n", true)]
    #[case("left", "b", "m", false)]
    #[case("right", "a", "n", false)]
    #[case("right", "b", "m", true)]
    #[case("full", "a", "n", false)]
    #[case("full", "b", "m", false)]
    fn preserves_outer_join_nullability(
        #[case] join: &str,
        #[case] alias: &str,
        #[case] column: &str,
        #[case] not_null: bool,
        #[values(false, true)] qualified: bool,
    ) -> testresult::TestResult {
        let (from, context) = TestParseContext::scalar_scope_from_sql(&format!(
            "(select 42 as n) a {join} join (select 43 as m) b on true"
        ))?;
        let with = WithClause::default();
        let scope = ScalarAnalysisContext::new(&with, &from);
        let alias = if qualified {
            Some(AliasName::from(alias))
        } else {
            None
        };
        let (expr, context) =
            scope.resolve_column_ref(context, alias.as_ref(), &ColumnName::from(column))?;
        assert_eq!(expr.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(expr.is_not_null(), Some(not_null));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }
}

impl ScalarAnalysisContext<'_> {
    fn resolve_returning_image_column(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        use crate::engine_specific::{Postgres18Feature, Postgres18Syntax};
        if let Some((target, role, nullable)) = self.merge_row_target(alias) {
            if matches!(alias.as_str(), "old" | "new") {
                context.postgres18_syntax_used(Postgres18Syntax::ReturningRowImages)?;
            }
            let (value, _) = self.resolve_source_column_ref(context, target, column)?;
            return Ok(Some(value.merge_record_column(role, nullable)));
        }
        if let Some((target, version)) = self.delete_row_target(alias)
            && let Some(source) = self.from_clause.destination_source(target)
        {
            context.postgres18_syntax_used(Postgres18Syntax::ReturningRowImages)?;
            let (value, _) = self.bind_source(context, &source, column)?;
            return Ok(Some(value.delete_row_column(version)));
        }
        Ok(None)
    }

    pub(crate) fn resolve_column<'context, TContext: ParseContext>(
        &self,
        context: &'context mut TContext,
        alias: Option<&AliasName>,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, &'context mut TContext), AnalysisError> {
        if let Some(expression) = self.resolve_declared_column(context, alias, column)? {
            return Ok((expression, context));
        }
        if let Some(alias) = alias {
            if self.qualified_only && context.dialect() == Dialect::sqlite() {
                let normalized = AliasName::from(alias.as_str().to_ascii_lowercase().as_str());
                if let Some((source, _)) = self.from_clause.resolve_alias(&normalized) {
                    let canonical = source
                        .canonical_relation_column(context, column)
                        .unwrap_or_else(|| column.clone());
                    return self.resolve_source_column_ref(context, &normalized, &canonical);
                }
            }
            if let Some(value) = self.resolve_returning_image_column(context, alias, column)? {
                return Ok((value, context));
            }

            match self.from_clause.namespace().resolve_name(alias) {
                Err(()) => {
                    let expression = QualifiedColumnFailure::ambiguous(
                        context,
                        &FullName::from_local_name(alias.as_str()),
                        column,
                    )?;
                    return Ok((expression, context));
                }
                Ok(Some(source)) => return self.bind_source(context, &source, column),
                Ok(None) => {
                    if self.notify_unbound_row_image(context, alias)? {
                        return Ok((UndefinedAliasColumnRef::unreported(alias, column), context));
                    }
                    if let Some(parent) = &self.parent {
                        return parent.resolve_column(context, Some(alias), column);
                    }
                    return self.resolve_source_column_ref(context, alias, column);
                }
            }
        }

        if self.qualified_only {
            let expr = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
            return Ok((expr, context));
        }
        if let Some(projected) =
            self.from_clause
                .joined_projection(context, &self.with_clause, column)?
        {
            self.grouping.reference();
            let expression = projected.bind(context, self)?;
            return Ok((expression, context));
        }
        let Ok(find_alias_result) =
            self.from_clause
                .namespace()
                .resolve_column(&*context, &self.with_clause, column)?
        else {
            let expr = ColumnReferenceExpr::ambiguous_column_resolved(context, column)?;
            return Ok((expr, context));
        };
        let alias = match find_alias_result {
            ::sql_fun_core::LookupResult::Found(alias) => alias,
            ::sql_fun_core::LookupResult::Unknown => {
                let expr = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
                return Ok((expr, context));
            }
            ::sql_fun_core::LookupResult::NotFound => {
                if let Some(columns) = &self.output_aliases {
                    match columns.resolve_output(column, context) {
                        Ok(Some(index)) => {
                            return Ok((
                                columns.output_reference(index).expect("bound output"),
                                context,
                            ));
                        }
                        Err(()) => {
                            let expression =
                                ColumnReferenceExpr::ambiguous_column_resolved(context, column)?;
                            return Ok((expression, context));
                        }
                        Ok(None) => {}
                    }
                }
                let alias = AliasName::from(column.as_str());
                if self
                    .from_clause
                    .namespace()
                    .resolve_name(&alias)
                    .is_ok_and(|source| source.is_none())
                    && self.notify_unbound_row_image(context, &alias)?
                {
                    return Ok((
                        WholeRowExpr::not_found_after_notification(&FullName::from_local_name(
                            alias.as_str(),
                        )),
                        context,
                    ));
                }
                if let Some(parent) = &self.parent {
                    return parent.resolve_column(context, None, column);
                }
                let expr = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
                return Ok((expr, context));
            }
        };
        self.bind_source(context, &alias, column)
    }
}

#[cfg(test)]
mod test_source_columns_remain_visible_in_nested_scalar_expressions {
    use crate::{
        context::MutSourcePathWithSpan,
        names::ColumnName,
        sem::{PgBuiltInType, SourcePathWithSpan, select_statement::SelectStatementNode},
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext, source_span},
    };

    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    fn source_columns_remain_visible_in_nested_scalar_expressions(
        source_span: SourcePathWithSpan,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.set_source_path_with_span(&source_span);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::bool());
        let syn = SynBuilder::new().parse(
            "select case when true then coalesce(s.n) else n end as result \
                 from (select 42 as n) s where s.n is not null",
        );
        let (select, context) = SelectStatementNode::analyze_select_node(
            context,
            syn.as_select_stmt().as_inner().expect("select statement"),
            &IVec::default(),
        )?;
        let column = select
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result column");
        assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }

    #[rstest::rstest]
    #[case("n")]
    #[case("missing")]
    #[case("unknown.n")]
    #[case("a.missing")]
    fn unresolved_column_reports_a_problem(
        source_span: SourcePathWithSpan,
        #[case] expression: &str,
    ) -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.set_source_path_with_span(&source_span);
        let syn = SynBuilder::new().parse(&format!(
            "select {expression} as result from (select 42 as n) a, (select 43 as n) b"
        ));
        let (select, context) = SelectStatementNode::analyze_select_node(
            context,
            syn.as_select_stmt().as_inner().expect("select statement"),
            &IVec::default(),
        )?;
        let column = select
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result column");
        assert_eq!(column.get_type_cloned(), None);
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

#[cfg(test)]
mod test_retains_distinct_missing_and_ambiguous_lookup_outcomes {

    use crate::{ast::Dialect, test_helpers::delete::analyze};

    #[test]
    fn retains_distinct_missing_and_ambiguous_lookup_outcomes() {
        let (statement, _) = analyze(
            "DELETE FROM items USING items AS other RETURNING id, absent",
            Dialect::postgres(),
        );
        let columns = statement.as_delete().unwrap().result_columns();
        let ambiguous = columns.get_column_at(0).unwrap().expression().unwrap();
        let missing = columns.get_column_at(1).unwrap().expression().unwrap();
        assert_matches::assert_matches!(
            ambiguous,
            crate::sem::SemScalarExpr::ColumnRef(
                crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_)
            )
        );
        assert_matches::assert_matches!(
            missing,
            crate::sem::SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(
                _
            ))
        );
    }
}

#[cfg(test)]
mod test_column_lookup_scope {
    use crate::{
        ast::Dialect,
        sem::{AnalysisError, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
        test_helpers::query::column_scope_query,
    };

    #[derive(Default, Debug)]
    struct Bindings {
        tables: Vec<String>,
        unresolved: usize,
        ambiguous: usize,
        outputs: usize,
    }

    impl crate::sem::traversal::SemanticVisitorMut for Bindings {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if expression.output_index().is_some() {
                self.outputs += 1;
            }
            if let SemScalarExpr::ColumnRef(reference) = expression {
                if let Some(table) = reference.table_column_reference() {
                    self.tables
                        .push(table.relation().local_name().as_str().to_owned());
                    assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
                } else if matches!(reference, ColumnReferenceExpr::AmbiguousColumnRef(_)) {
                    self.ambiguous += 1;
                    assert_eq!(expression.get_type(), None);
                } else if matches!(
                    reference,
                    ColumnReferenceExpr::UndefinedColumn(_)
                        | ColumnReferenceExpr::UndefinedTableOrView(_)
                        | ColumnReferenceExpr::UndefinedAlias(_)
                        | ColumnReferenceExpr::UndefinedFunction(_)
                ) {
                    self.unresolved += 1;
                    assert_eq!(expression.get_type(), None);
                }
            }
            Ok(())
        }
    }

    #[rstest::rstest]
    #[case::original("SELECT (SELECT id FROM missing.items) FROM outer_items;")]
    #[case::competing_known("SELECT (SELECT id FROM known_items, missing.items) FROM outer_items")]
    #[case::unknown_first("SELECT (SELECT id FROM missing.items, known_items) FROM outer_items")]
    #[case::multiple_unknown(
        "SELECT (SELECT id FROM missing.items a, missing.other b) FROM outer_items"
    )]
    #[case::join("SELECT (SELECT id FROM known_items CROSS JOIN missing.items) FROM outer_items")]
    #[case::join_unknown_first(
        "SELECT (SELECT id FROM missing.items CROSS JOIN known_items) FROM outer_items"
    )]
    #[case::nested_unknown("SELECT (SELECT (SELECT id) FROM missing.items) FROM outer_items")]
    #[case::deep_unknown(
        "SELECT (SELECT (SELECT id FROM missing.items) FROM local_items) FROM outer_items"
    )]
    #[case::shadowed_alias(
        "SELECT (SELECT outer_items.id FROM missing.items outer_items) FROM outer_items"
    )]
    #[case::qualified_missing("SELECT (SELECT items.id FROM missing.items) FROM outer_items")]
    fn unresolved_local_relation_blocks_outer_binding(#[case] sql: &str) {
        let (mut query, context) = column_scope_query(sql, Dialect::postgres());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert!(bindings.tables.is_empty(), "{bindings:?}");
        assert_eq!(bindings.unresolved, 1, "{bindings:?}");
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "relation_not_found")
        );
    }

    #[rstest::rstest]
    #[case::missing("SELECT (SELECT id FROM missing()) FROM outer_items")]
    #[case::competing_known("SELECT (SELECT id FROM known_items, missing()) FROM outer_items")]
    #[case::unknown_first("SELECT (SELECT id FROM missing(), known_items) FROM outer_items")]
    #[case::join("SELECT (SELECT id FROM known_items CROSS JOIN missing()) FROM outer_items")]
    #[case::nested("SELECT (SELECT (SELECT id) FROM missing()) FROM outer_items")]
    fn unresolved_table_function_blocks_outer_binding(#[case] sql: &str) {
        let (mut query, context) = column_scope_query(sql, Dialect::postgres());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert!(bindings.tables.is_empty(), "{bindings:?}");
        assert_eq!(bindings.unresolved, 1, "{bindings:?}");
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "table_function_not_found")
        );
    }

    #[rstest::rstest]
    #[case::resolved_absence("SELECT (SELECT id FROM local_items) FROM outer_items", "outer_items")]
    #[case::empty_from("SELECT (SELECT id) FROM outer_items", "outer_items")]
    #[case::nested_absence(
        "SELECT (SELECT (SELECT id FROM local_items)) FROM outer_items",
        "outer_items"
    )]
    #[case::local_binding("SELECT (SELECT id FROM known_items) FROM outer_items", "known_items")]
    #[case::local_shadowing(
        "SELECT (SELECT outer_items.id FROM known_items outer_items) FROM outer_items",
        "known_items"
    )]
    fn conclusive_lookup_preserves_binding(#[case] sql: &str, #[case] relation: &str) {
        let (mut query, context) = column_scope_query(sql, Dialect::postgres());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert_eq!(bindings.tables, [relation]);
        assert_eq!(bindings.unresolved, 0);
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn explicit_outer_qualification_bypasses_unrelated_unknown_source() {
        let (mut query, context) = column_scope_query(
            "SELECT (SELECT outer_items.id FROM missing.items) FROM outer_items",
            Dialect::postgres(),
        );
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert_eq!(bindings.tables, ["outer_items"]);
        assert_eq!(bindings.unresolved, 0);
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "relation_not_found")
        );
    }

    #[rstest::rstest]
    #[case::known("known_items a, known_items b")]
    #[case::unknown_first("missing.items, known_items a, known_items b")]
    #[case::unknown_last("known_items a, known_items b, missing.items")]
    #[case::join("known_items a CROSS JOIN known_items b CROSS JOIN missing.items")]
    fn established_ambiguity_survives_unknown_sources(#[case] from: &str) {
        let sql = format!("SELECT (SELECT id FROM {from}) FROM outer_items");
        let (mut query, context) = column_scope_query(&sql, Dialect::postgres());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert!(bindings.tables.is_empty());
        assert_eq!(bindings.ambiguous, 1);
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "ambiguous_column")
        );
    }

    #[test]
    fn conclusive_missing_column_remains_unresolved() {
        let (mut query, context) =
            column_scope_query("SELECT absent FROM local_items", Dialect::postgres());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert!(bindings.tables.is_empty());
        assert_eq!(bindings.unresolved, 1);
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[test]
    fn unknown_input_does_not_bind_grouping_output_alias() {
        let (mut query, _) = column_scope_query(
            "SELECT 1 AS id FROM missing.items GROUP BY id",
            Dialect::postgres(),
        );
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert_eq!(bindings.unresolved, 1);
        assert_eq!(bindings.outputs, 0);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::group("SELECT (SELECT id FROM (missing_items) AS g) FROM outer_items")]
    #[case::group_join(
        "SELECT (SELECT id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
    )]
    #[case::qualified_group(
        "SELECT (SELECT g.id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
    )]
    fn sqlite_group_preserves_inconclusive_lookup(#[case] sql: &str) {
        let (mut query, context) = column_scope_query(sql, Dialect::sqlite());
        let mut bindings = Bindings::default();
        query.walk(&mut bindings).unwrap();
        assert!(bindings.tables.is_empty());
        assert_eq!(bindings.unresolved, 1);
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "relation_not_found")
        );
    }
}

#[cfg(test)]
mod test_preserves_failed_column_resolution {
    use crate::{
        ast::Dialect,
        sem::{AnalysisError, ColumnReferenceExpr, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::missing_order("SELECT 1 ORDER BY missing", false)]
    #[case::ambiguous_order("SELECT 1 AS x, 2 AS x ORDER BY x", true)]
    #[case::having_has_no_alias("SELECT 1 AS x HAVING x IS NULL", false)]
    fn preserves_failed_column_resolution(#[case] sql: &str, #[case] ambiguous: bool) {
        let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert!(context.reported_problem_count() > 0);
        struct Failures {
            missing: usize,
            ambiguous: usize,
        }
        impl crate::sem::traversal::SemanticVisitorMut for Failures {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                match expression {
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_)) => {
                        self.missing += 1
                    }
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_)) => {
                        self.ambiguous += 1
                    }
                    _ => {}
                }
                Ok(())
            }
        }
        let mut failures = Failures {
            missing: 0,
            ambiguous: 0,
        };
        query.walk(&mut failures).unwrap();
        if ambiguous {
            assert_eq!(failures.ambiguous, 1);
        } else {
            assert_eq!(failures.missing, 1);
        }
    }
}

impl ScalarAnalysisContext<'_> {
    pub(crate) fn bind_source<'context, C: ParseContext>(
        &self,
        context: &'context mut C,
        source: &QualifiedSource<'_>,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, &'context mut C), AnalysisError> {
        self.grouping.reference();
        self.validate_rule_reference(context, source.alias())?;
        let expression = source.bind(context, &self.with_clause, column)?;
        Ok((expression, context))
    }
}

impl ScalarAnalysisContext<'_> {
    fn resolve_source_column_ref<'context, TParseContext>(
        &self,
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        self.grouping.reference();
        let Some((source, nullability)) = self.from_clause.resolve_alias(alias) else {
            let expr = UndefinedAliasColumnRef::new_with_report(context, alias, column)?;
            return Ok((expr, context));
        };
        self.validate_rule_reference(context, alias)?;
        source.create_column_ref(context, &self.with_clause, alias, column, nullability)
    }
}

impl ScalarAnalysisContext<'_> {
    pub(crate) fn resolve_whole_row(
        &self,
        context: &mut impl ParseContext,
        requested: &FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut relation = requested.clone();
        if let Some(catalog) = requested.catalog() {
            let Some(database) = context.metadata().database_name() else {
                return WholeRowExpr::database_required(context, requested);
            };
            if catalog.as_str() != database {
                return WholeRowExpr::cross_database(context, requested);
            }
            relation = FullName::new(&None, &requested.schema().cloned(), requested.name());
        }
        self.resolve_whole_row_in_scope(context, &relation, requested, 0)
    }

    fn resolve_whole_row_in_scope(
        &self,
        context: &mut impl ParseContext,
        relation: &FullName,
        requested: &FullName,
        levels_up: usize,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if let Some(table) = &self.declared_table {
            let name = table.identity.full_name();
            if relation.name() == name.name()
                && (relation.schema().is_none() || relation.schema() == name.schema())
            {
                return self.declared_whole_row(context, requested, levels_up);
            }
        }
        let source = if relation.schema().is_some() {
            if self.qualified_only {
                Ok(None)
            } else {
                self.from_clause.qualified_source(context, relation)
            }
        } else {
            self.from_clause
                .namespace()
                .resolve_name(&AliasName::from(relation.name().as_str()))
        };
        match source {
            Ok(Some(source)) => {
                self.grouping.reference();
                self.validate_rule_reference(context, source.alias())?;
                source.bind_whole_row(context, &self.with_clause, requested, levels_up)
            }
            Err(()) => WholeRowExpr::ambiguous(context, requested),
            Ok(None) => {
                if relation.schema().is_none()
                    && self.notify_unbound_row_image(
                        context,
                        &AliasName::from(relation.name().as_str()),
                    )?
                {
                    return Ok(WholeRowExpr::not_found_after_notification(requested));
                }
                if let Some(parent) = &self.parent {
                    parent.resolve_whole_row_in_scope(context, relation, requested, levels_up + 1)
                } else {
                    WholeRowExpr::not_found(context, requested)
                }
            }
        }
    }
}

#[cfg(test)]
mod test_retains_failed_lookup {
    use crate::{
        sem::PgBuiltInType,
        test_helpers::{
            query::expression, query_whole_rows as test_helpers, query_whole_rows::catalog_query,
        },
    };

    #[rstest::rstest]
    #[case::missing(
        "SELECT identity(missing.*) FROM pg_catalog.pg_type t",
        "column_source_not_found"
    )]
    #[case::missing_relation("SELECT identity(x.*) FROM missing x", "column_source_not_found")]
    #[case::hidden(
        "SELECT identity(pg_catalog.pg_type.*) FROM pg_catalog.pg_type t",
        "column_source_not_found"
    )]
    #[case::ambiguous(
        "SELECT identity(t.*) FROM pg_catalog.pg_type t CROSS JOIN pg_catalog.pg_attribute t",
        "column_source_ambiguous"
    )]
    fn retains_failed_lookup(#[case] sql: &str, #[case] code: &str) {
        let (query, context) = catalog_query(sql);
        let row = expression(&query).as_function_call().unwrap().args()[0]
            .value()
            .as_whole_row()
            .unwrap();
        assert_eq!(row.failure_code(), Some(code));
        assert!(row.alias().is_none());
        assert!(
            expression(&query).as_function_call().unwrap().args()[0]
                .value()
                .get_type()
                .is_none()
        );
        test_helpers::assert_problem(&context, code);
    }

    #[test]
    fn correlation_records_the_selected_query_level() {
        let (query, context) =
            catalog_query("SELECT (SELECT record_arg(x.*)) FROM (SELECT 1 AS n) x");
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(test_helpers::row_levels(query), vec![1]);
    }

    #[test]
    fn local_source_shadows_an_outer_source_with_the_same_alias() {
        let (query, context) = catalog_query(
            "SELECT (SELECT record_arg(x.*) FROM (SELECT 2 AS n) x) FROM (SELECT 1 AS n) x",
        );
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(test_helpers::row_levels(query), vec![0]);
    }

    #[rstest::rstest]
    #[case::view("SELECT (SELECT (r.*).x) FROM v AS r(x)", 1)]
    #[case::derived("SELECT (SELECT (r.*).x) FROM (SELECT 1 AS n) AS r(x)", 1)]
    #[case::shadow("SELECT (SELECT (r.*).y FROM v AS r(y)) FROM v AS r(x)", 0)]
    fn occurrence_aliases_follow_scope(#[case] sql: &str, #[case] level: usize) {
        let (query, context) = test_helpers::alias_query(sql);
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(test_helpers::row_levels(query), vec![level]);
    }

    #[test]
    fn occurrence_alias_hides_original_relation() {
        let (query, context) = test_helpers::alias_query("SELECT (v.*).n FROM v AS r(x)");
        assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
        assert_eq!(context.problems()[0].code(), "column_source_not_found");
        assert!(expression(&query).get_type().is_none());
    }
}
