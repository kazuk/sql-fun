use crate::sem::{
    AnalysisError, ColumnName, ColumnReferenceExpr, FromClause, ParseContext, SemScalarExpr,
    WithClause, data_source::AliasName, scalar_expr::column_ref::UndefinedAliasColumnRef,
};

use std::borrow::Cow;

/// Expression-local scope used during scalar semantic analysis.
///
/// This is separate from [`crate::sem::ParseContext`], which provides schema
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
    output_aliases: Option<crate::sem::ResultColumnCollection>,
    delete_returning_target: Option<AliasName>,
    qualified_only: bool,
    rule: Option<RuleScope>,
    trigger_namespace: Option<crate::sem::SchemaName>,
}

#[derive(Debug, Clone)]
struct RuleScope {
    identity: crate::sem::RuleIdentity,
    event: crate::sem::RuleEvent,
    forbidden: Option<&'static str>,
}

impl<'a> ScalarAnalysisContext<'a> {
    pub(crate) const fn grouping_scope(&self) -> &super::grouping::GroupingScope {
        &self.grouping
    }
    pub(crate) const fn set_grouping_query(&mut self, id: usize) {
        self.grouping.set_query(id);
    }

    pub(crate) fn with_grouping_capture(
        &self,
        capture: &super::grouping::ReferenceCapture,
    ) -> Self {
        let mut scope = self.clone_scope();
        scope.add_grouping_capture(capture);
        scope
    }
    fn add_grouping_capture(&mut self, capture: &super::grouping::ReferenceCapture) {
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

    pub(crate) fn with_added_source(&self, source: &crate::sem::DataSource) -> Self {
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
    ) -> Result<Vec<crate::sem::data_source::ProjectionColumn<'_>>, AnalysisError> {
        if !matches!(
            self.from_clause.namespace().resolve_name(qualifier),
            Ok(None)
        ) {
            self.from_clause
                .projection_names(context, &self.with_clause, Some(qualifier))
        } else if let Some(parent) = &self.parent {
            parent.projection_names(context, qualifier)
        } else {
            Ok(Vec::new())
        }
    }

    pub(crate) fn relation_projection_names(
        &self,
        context: &mut impl ParseContext,
        relation: &crate::sem::FullName,
    ) -> Result<Vec<crate::sem::data_source::ProjectionColumn<'_>>, AnalysisError> {
        match self.from_clause.qualified_source(context, relation) {
            Ok(Some(source)) => source.projection_columns(context, &self.with_clause),
            Err(()) => self.ambiguous_relation_projection(context),
            Ok(None) => self.parent_relation_projection_names(context, relation),
        }
    }

    fn ambiguous_relation_projection(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Vec<crate::sem::data_source::ProjectionColumn<'_>>, AnalysisError> {
        context.report_analysis_problem(crate::sem::AnalysisProblem::query(
            crate::sem::QueryProblem::ambiguous_reference("star source"),
        ))?;
        Ok(Vec::new())
    }

    fn parent_relation_projection_names(
        &self,
        context: &mut impl ParseContext,
        relation: &crate::sem::FullName,
    ) -> Result<Vec<crate::sem::data_source::ProjectionColumn<'_>>, AnalysisError> {
        if let Some(parent) = &self.parent {
            parent.relation_projection_names(context, relation)
        } else {
            Ok(Vec::new())
        }
    }

    pub(crate) fn rule(
        with: &'a WithClause,
        from: &'a FromClause,
        identity: &crate::sem::RuleIdentity,
        event: crate::sem::RuleEvent,
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
                    context.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                        &crate::sem::problem::RuleProblem::definition(
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
    pub(crate) fn rule_identity(&self) -> Option<&crate::sem::RuleIdentity> {
        if let Some(rule) = &self.rule {
            Some(&rule.identity)
        } else if let Some(parent) = &self.parent {
            parent.rule_identity()
        } else {
            None
        }
    }
    pub(crate) fn direct_rule_identity(&self) -> Option<&crate::sem::RuleIdentity> {
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
            context.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                &crate::sem::problem::RuleProblem::definition(&rule.identity, detail),
            ))?;
        }
        Ok(())
    }

    pub(crate) const fn trigger(with: &'a WithClause, from: &'a FromClause) -> Self {
        let mut scope = Self::new(with, from);
        scope.qualified_only = true;
        scope
    }
    pub(crate) fn set_trigger_namespace(&mut self, schema: Option<&crate::sem::SchemaName>) {
        self.trigger_namespace = schema.cloned();
    }
    fn trigger_namespace(&self) -> Option<&crate::sem::SchemaName> {
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
        name: &crate::sem::FullName,
    ) -> Result<crate::sem::FullName, AnalysisError> {
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
            context.report_analysis_problem(crate::sem::AnalysisProblem::trigger(
                &crate::sem::problem::TriggerProblem::options(
                    name,
                    "non-TEMP SQLite triggers can reference only their own database",
                ),
            ))?;
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
            qualified_only: self.qualified_only,
            rule: self.rule.clone(),
            trigger_namespace: self.trigger_namespace.clone(),
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
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
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
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
        }
    }

    pub(crate) fn for_merge_on(
        with_clause: &'a WithClause,
        target: &crate::sem::DataSource,
        source: &crate::sem::DataSource,
    ) -> Self {
        Self::with_owned_from_clause(with_clause, FromClause::for_merge_on(target, source))
    }

    pub(crate) fn for_merge_when(
        with_clause: &'a WithClause,
        target: &crate::sem::DataSource,
        source: &crate::sem::DataSource,
        kind: crate::sem::MergeMatchKind,
    ) -> Self {
        Self::with_owned_from_clause(
            with_clause,
            FromClause::for_merge_when(target, source, kind),
        )
    }

    pub(crate) fn for_merge_returning(
        context: &impl ParseContext,
        with_clause: &'a WithClause,
        target: &crate::sem::DataSource,
        source: &crate::sem::DataSource,
        source_nullable: bool,
        old_nullable: bool,
        new_nullable: bool,
    ) -> Self {
        Self::with_owned_from_clause(
            with_clause,
            FromClause::for_merge_returning(
                target,
                source,
                source_nullable,
                old_nullable,
                new_nullable,
                postgres_18_or_later(context),
            ),
        )
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
        let Some(version) = context.metadata().engine_version() else {
            return;
        };
        let Ok(version) = version
            .to_string()
            .parse::<::sql_fun_core::PostgresVersion>()
        else {
            return;
        };
        if version.major() >= 18 {
            self.delete_returning_target = Some(target.clone());
        }
    }

    pub(crate) fn delete_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, crate::sem::DeleteRowVersion)> {
        if self.from_clause.contains_alias(alias) {
            return None;
        }
        let version = crate::sem::DeleteRowVersion::from_alias(alias.as_str())?;
        Some((self.delete_returning_target.as_ref()?, version))
    }
    pub(crate) fn set_output_aliases(&mut self, columns: &crate::sem::ResultColumnCollection) {
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
    pub(crate) fn resolve_lock(&self, name: &str) -> crate::sem::QueryNameResolution {
        let alias = AliasName::from(name);
        match self.from_clause.namespace().resolve_name(&alias) {
            Ok(Some(_)) => crate::sem::QueryNameResolution::source(&alias),
            Ok(None) => crate::sem::QueryNameResolution::not_found(),
            Err(()) => crate::sem::QueryNameResolution::ambiguous(),
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
        context: &impl crate::sem::ParseContext,
    ) -> crate::sem::QueryNameResolution {
        if context.dialect() == crate::ast::Dialect::sqlite() {
            return self
                .windows
                .iter()
                .rposition(|candidate| candidate == name)
                .map_or_else(
                    crate::sem::QueryNameResolution::not_found,
                    crate::sem::QueryNameResolution::window,
                );
        }
        let mut candidates = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.as_str() == name);
        let Some((index, _)) = candidates.next() else {
            return crate::sem::QueryNameResolution::not_found();
        };
        if candidates.next().is_some() {
            crate::sem::QueryNameResolution::ambiguous()
        } else {
            crate::sem::QueryNameResolution::window(index)
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
    ) -> Option<(&AliasName, crate::sem::data_source::MergeRecordRole, bool)> {
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
            qualified_only: false,
            rule: None,
            trigger_namespace: None,
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
        qualifier: &crate::sem::FullName,
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
            relation =
                crate::sem::FullName::new(&None, &qualifier.schema().cloned(), qualifier.name());
        }
        self.resolve_qualified_in_scope(context, &relation, qualifier, column)
    }

    fn resolve_qualified_in_scope(
        &self,
        context: &mut impl ParseContext,
        relation: &crate::sem::FullName,
        requested: &crate::sem::FullName,
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

    pub(crate) fn resolve_column<'context, TContext: crate::sem::ParseContext>(
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
            if let Some((target, role, nullable)) = self.merge_row_target(alias) {
                let (value, context) = self.resolve_source_column_ref(context, target, column)?;
                return Ok((value.merge_record_column(role, nullable), context));
            }
            if let Some((target, version)) = self.delete_row_target(alias)
                && let Some(source) = self.from_clause.destination_source(target)
            {
                let (value, context) = self.bind_source(context, &source, column)?;
                return Ok((value.delete_row_column(version), context));
            }

            match self.from_clause.namespace().resolve_name(alias) {
                Err(()) => {
                    let expression = super::column_ref::QualifiedColumnFailure::ambiguous(
                        context,
                        &crate::sem::FullName::from_local_name(alias.as_str()),
                        column,
                    )?;
                    return Ok((expression, context));
                }
                Ok(Some(source)) => return self.bind_source(context, &source, column),
                Ok(None) => {
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
        source: &crate::sem::data_source::QualifiedSource<'_>,
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
        sem::{
            ColumnDefinition, ColumnName, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr,
            SubQueryColumnRef, WithClause, data_source::AliasName,
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
        TParseContext: crate::sem::ParseContext,
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
        requested: &crate::sem::FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        use crate::sem::{FullName, WholeRowExpr};
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
        relation: &crate::sem::FullName,
        requested: &crate::sem::FullName,
        levels_up: usize,
    ) -> Result<SemScalarExpr, AnalysisError> {
        use crate::sem::WholeRowExpr;
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
                if let Some(parent) = &self.parent {
                    parent.resolve_whole_row_in_scope(context, relation, requested, levels_up + 1)
                } else {
                    WholeRowExpr::not_found(context, requested)
                }
            }
        }
    }
}
