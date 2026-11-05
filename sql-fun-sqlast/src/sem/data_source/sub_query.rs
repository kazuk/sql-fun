use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, Nullable, SelectStatement,
        WithClause,
        scalar_expr::{ColumnReferenceExpr, SubQueryColumnRef},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubQueryDataSource {
    select_statement: SelectStatement,
    alias: AliasSpec,
}

impl SubQueryDataSource {
    pub(super) fn new_data_source(s: &SelectStatement, alias: &AliasSpec) -> DataSource {
        DataSource::SubQuery(Box::new(Self {
            select_statement: s.clone(),
            alias: alias.clone(),
        }))
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        _context: &TParseContext,
        _with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        let result_columns = self.select_statement.result_columns();
        let result_column = if let Some(index) = self.alias.derived_column_index(column) {
            result_columns.get_column_at(index)
        } else {
            result_columns.get_column_with_skip_count(column, self.alias.len_derived_column())
        };
        if result_column.is_some() {
            names.push(self.alias.clone());
        }
    }
}

#[cfg(test)]
mod test_visible_columns_retain_qualified_binding {
    use crate::{
        ast::Dialect,
        sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::union(
        "SELECT renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)",
        "SELECT x.renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)"
    )]
    #[case::simple(
        "SELECT renamed FROM (SELECT 1 AS n) x(renamed)",
        "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed)"
    )]
    #[case::partial(
        "SELECT b FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
        "SELECT x.b FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
    )]
    #[case::renamed_partial(
        "SELECT renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
        "SELECT x.renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
    )]
    #[case::hidden_name_does_not_compete(
        "SELECT n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y",
        "SELECT y.n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y"
    )]
    #[case::local_shadowing(
        "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)",
        "SELECT (SELECT y.renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)"
    )]
    #[case::correlated(
        "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)",
        "SELECT (SELECT x.renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)"
    )]
    #[case::hidden_local_name_allows_correlation(
        "SELECT (SELECT n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x",
        "SELECT (SELECT x.n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x"
    )]
    #[case::qualified_disambiguates(
        "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
        "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)"
    )]
    fn visible_columns_retain_qualified_binding(#[case] sql: &str, #[case] qualified: &str) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let (expected, context) = resolve(qualified, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let column = query.result_columns().get_column_at(0).unwrap();
        let expected_column = expected.result_columns().get_column_at(0).unwrap();
        // Equality includes the source alias, requested name and retained column definition.
        assert_eq!(column.expression(), expected_column.expression());
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(
            column.expression().unwrap().is_not_null(),
            expected_column.expression().unwrap().is_not_null()
        );
        assert_eq!(column.column_name(), expected_column.column_name());
    }

    #[rstest::rstest]
    #[case::simple("SELECT n FROM (SELECT 1 AS n) x(renamed)")]
    #[case::union("SELECT n FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)")]
    fn hidden_original_name_is_unresolved(#[case] sql: &str) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        assert_eq!(
            context.reported_problems()[0].code(),
            "column_not_found_in_source"
        );
        assert_matches::assert_matches!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression(),
            Some(SemScalarExpr::ColumnRef(
                ColumnReferenceExpr::UndefinedColumn(_)
            ))
        );
    }
}

impl SubQueryDataSource {
    pub const fn select_statement(&self) -> &SelectStatement {
        &self.select_statement
    }

    pub(super) fn create_column_ref<TParseContext>(
        &self,
        context: &mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        nullability: Nullable,
    ) -> Result<ColumnReferenceExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let result_columns = self.select_statement.result_columns();
        let column_def = if let Some(index) = self.alias.derived_column_index(column) {
            if let Some(result_column_ref) = result_columns.get_column_at(index) {
                let column_def = result_column_ref.create_column_definition();
                Some(column_def)
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::subquery_not_have_column_for_derived_column_name(),
                )?;
                None
            }
        } else if let Some(result_column_ref) =
            result_columns.get_column_with_skip_count(column, self.alias.len_derived_column())
        {
            let column_def = result_column_ref.create_column_definition();
            Some(column_def)
        } else {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_subquery(
                alias, column,
            ))?;
            None
        };

        Ok(ColumnReferenceExpr::SubQueryColumn(SubQueryColumnRef::new(
            alias,
            column,
            column_def.as_ref(),
            nullability,
        )))
    }
}

#[cfg(test)]
mod test_renamed_column_retains_type_and_nullability {
    use crate::{
        ast::Dialect,
        sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::non_null("1", true)]
    #[case::nullable("NULL::int4", false)]
    fn renamed_column_retains_type_and_nullability(#[case] value: &str, #[case] not_null: bool) {
        let sql = format!("SELECT renamed FROM (SELECT {value} AS n) x(renamed)");
        let (query, context) = resolve(&sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(
            column.column_name(),
            &Some(crate::names::ColumnName::from("renamed"))
        );
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(not_null));
        assert_matches::assert_matches!(
            column.expression(),
            Some(SemScalarExpr::ColumnRef(
                ColumnReferenceExpr::SubQueryColumn(_)
            ))
        );
    }
}

impl SubQueryDataSource {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.source_outputs(&self.select_statement.result_columns().names());
        self.select_statement.walk(visitor)?;
        visitor.derived_binding(&self.alias, &self.select_statement.result_columns().names());
        visitor.derived(self.alias.name().as_str(), &self.alias.inference_columns());
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.source_outputs(&self.select_statement.result_columns().names());
        self.select_statement.visit(visitor)?;
        visitor.derived_binding(&self.alias, &self.select_statement.result_columns().names());
        visitor.derived(self.alias.name().as_str(), &self.alias.inference_columns());
        Ok(())
    }
}

impl SubQueryDataSource {
    pub(super) fn projection_column_names(&self) -> Vec<ColumnName> {
        self.select_statement.result_columns().names()
    }
}
