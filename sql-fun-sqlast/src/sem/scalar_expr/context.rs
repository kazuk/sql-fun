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
    output_aliases: Option<crate::sem::ResultColumnCollection>,
}

impl<'a> ScalarAnalysisContext<'a> {
    fn clone_scope(&self) -> Self {
        Self {
            windows: self.windows.clone(),
            output_aliases: self.output_aliases.clone(),
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
            output_aliases: None,
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
            output_aliases: None,
        }
    }

    pub(crate) fn set_output_aliases(&mut self, columns: &crate::sem::ResultColumnCollection) {
        self.output_aliases = Some(columns.clone());
    }
    pub(crate) fn has_input_column(
        &self,
        context: &crate::sem::Resolution<'_>,
        column: &ColumnName,
    ) -> bool {
        match self
            .from_clause
            .find_alias_resolved(column, context, &self.with_clause)
        {
            Ok(None) => self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.has_input_column(context, column)),
            _ => true,
        }
    }
    pub(crate) fn resolve_lock(&self, name: &str) -> crate::sem::QueryNameResolution {
        let alias = AliasName::from(name);
        if self.from_clause.resolve_alias(&alias).is_some() {
            crate::sem::QueryNameResolution::source(&alias)
        } else {
            crate::sem::QueryNameResolution::not_found()
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
        dialect: crate::ast::Dialect,
    ) -> crate::sem::QueryNameResolution {
        if dialect == crate::ast::Dialect::sqlite() {
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
}

impl ScalarAnalysisContext<'_> {
    /// Resolve a column reference in this scope, reporting missing or ambiguous names.
    ///
    /// An alias restricts lookup to that data source. Without an alias, the column
    /// must identify exactly one source. Source nullability is preserved in the
    /// resulting expression.
    pub fn resolve_column_ref<TParseContext>(
        &self,
        context: TParseContext,
        alias: Option<&AliasName>,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        self.resolve_column(context, alias, column)
    }

    pub(crate) fn resolve_column<TContext: crate::sem::resolution::ResolutionContext>(
        &self,
        mut context: TContext,
        alias: Option<&AliasName>,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, TContext), AnalysisError> {
        if let Some(alias) = alias {
            if self.from_clause.resolve_alias(alias).is_none()
                && let Some(parent) = &self.parent
            {
                return parent.resolve_column(context, Some(alias), column);
            }
            return self.resolve_source_column_ref(context, alias, column);
        }

        let Ok(find_alias_result) =
            self.from_clause
                .find_alias_resolved(column, &context, &self.with_clause)
        else {
            let expr = ColumnReferenceExpr::ambiguous_column_resolved(&mut context, column)?;
            return Ok((expr, context));
        };
        let Some(alias) = find_alias_result else {
            if let Some(columns) = &self.output_aliases {
                match columns.resolve_output(column, crate::ast::Dialect::sqlite()) {
                    Ok(Some(index)) => {
                        return Ok((
                            columns.output_reference(index).expect("bound output"),
                            context,
                        ));
                    }
                    Err(()) => {
                        let expression =
                            ColumnReferenceExpr::ambiguous_column_resolved(&mut context, column)?;
                        return Ok((expression, context));
                    }
                    Ok(None) => {}
                }
            }
            if let Some(parent) = &self.parent {
                return parent.resolve_column(context, None, column);
            }
            let expr = ColumnReferenceExpr::undefined_column_resolved(&mut context, column)?;
            return Ok((expr, context));
        };
        self.resolve_source_column_ref(context, alias.name(), column)
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
    fn resolve_source_column_ref<TParseContext>(
        &self,
        mut context: TParseContext,
        alias: &AliasName,
        column: &ColumnName,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::resolution::ResolutionContext,
    {
        let Some((source, nullability)) = self.from_clause.resolve_alias(alias) else {
            let expr = UndefinedAliasColumnRef::new_with_report(&mut context, alias, column)?;
            return Ok((expr, context));
        };
        source.create_column_ref(context, &self.with_clause, alias, column, nullability)
    }
}
