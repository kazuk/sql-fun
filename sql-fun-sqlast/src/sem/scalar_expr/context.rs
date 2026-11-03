use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName, FullName, SchemaName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnReferenceExpr, DataSource, DeleteRowVersion,
        FromClause, MergeMatchKind, QueryNameResolution, QueryProblem, ResultColumnCollection,
        RuleEvent, RuleIdentity, SemScalarExpr, WholeRowExpr, WithClause,
        data_source::{MergeRecordRole, QualifiedSource, WildcardProjection},
        problem::{RuleProblem, TriggerProblem},
        scalar_expr::column_ref::UndefinedAliasColumnRef,
    },
};

use std::borrow::Cow;

use super::grouping::ReferenceCapture;

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
    grouping: super::grouping::GroupingScope,
    output_aliases: Option<ResultColumnCollection>,
    delete_returning_target: Option<AliasName>,
    postgres_returning: bool,
    qualified_only: bool,
    rule: Option<RuleScope>,
    trigger_namespace: Option<SchemaName>,
    projection_scope_id: Option<usize>,
}

#[derive(Debug, Clone)]
struct RuleScope {
    identity: RuleIdentity,
    event: RuleEvent,
    forbidden: Option<&'static str>,
}

impl ScalarAnalysisContext<'_> {
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
    #[rstest::rstest]
    #[should_panic(expected = "RETURNING destination row-image expressions")]
    fn unbound_images_reach_todo(#[values("old", "new")] name: &str) {
        let mut context =
            crate::test_helpers::TestParseContext::default().with_postgres_version("18");
        let with_clause = crate::sem::WithClause::default();
        let from_clause = crate::sem::FromClause::default();
        let mut scope = super::ScalarAnalysisContext::new(&with_clause, &from_clause);
        scope.set_postgres_returning(&context);
        let _ = scope.notify_unbound_row_image(&mut context, &crate::names::AliasName::from(name));
    }
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) const fn grouping_scope(&self) -> &super::grouping::GroupingScope {
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
        if context.dialect() != crate::ast::Dialect::sqlite() {
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
            windows: Vec::new(),
            grouping: super::grouping::GroupingScope::new(),
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
            windows: Vec::new(),
            grouping: super::grouping::GroupingScope::nested(parent.map(|scope| &scope.grouping)),
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
        self.postgres_returning = context.dialect() == crate::ast::Dialect::postgres();
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
        if context.dialect() != crate::ast::Dialect::postgres() {
            return;
        }
        self.postgres_returning = true;
        self.delete_returning_target = Some(target.clone());
    }

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
        if context.dialect() == crate::ast::Dialect::sqlite() {
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
            windows: Vec::new(),
            grouping: super::grouping::GroupingScope::new(),
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
    if context.dialect() != crate::ast::Dialect::postgres() {
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
        use super::column_ref::QualifiedColumnFailure;
        let mut relation = qualifier.clone();
        if let Some(catalog) = qualifier.catalog() {
            if context.dialect() == crate::ast::Dialect::sqlite() {
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
        use super::column_ref::QualifiedColumnFailure;
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
        if let Some(alias) = alias {
            if self.qualified_only && context.dialect() == crate::ast::Dialect::sqlite() {
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
                    let expression = super::column_ref::QualifiedColumnFailure::ambiguous(
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

#[cfg(test)]
mod test_resolve_column_ref {
    use crate::sem::scalar_expr::context::ScalarAnalysisContext;
    use crate::{
        names::{AliasName, ColumnName},
        sem::{
            ColumnDefinition, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr, SubQueryColumnRef,
            WithClause,
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
