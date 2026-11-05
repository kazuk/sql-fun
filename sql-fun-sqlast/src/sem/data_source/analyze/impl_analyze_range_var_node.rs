use crate::{
    ast::{FromItem, Relation},
    sem::{AnalysisError, DataSource, FromClause as SemFromClause, ParseContext, WithClause},
    syn::RangeVar,
};

impl DataSource {
    pub(super) fn analyze_range_var_node<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        range_var: RangeVar,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = FromItem::relation(Relation::try_from(&range_var)?);
        let outcome = Self::from_ast_source(
            &syntax,
            &mut context,
            with_clause,
            &SemFromClause::default(),
        );
        Ok((outcome?, context))
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_var_node {
    use crate::{
        ast::CreateTableMode,
        names::{ColumnName, FullName, SchemaName, TableName},
        sem::{
            CreateTable, CreateView, FromClause, ScalarConstExpr, SelectStatement, SemScalarExpr,
            SourcePathWithSpan, ViewLike, WithClause,
            data_source::analyze::impl_analyze_range_var_node::DataSource,
            select_statement::{ResultColumnCollection, SelectStatementNode},
        },
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };
    use ::testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_range_var_node_table(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        let with_clause = WithClause::default();
        let builder = SynBuilder::new();
        let name = FullName::from_local_name("table1");
        let alias = builder.alias("t1", &[]);
        let syn = builder.range_var(&name, &alias.into());

        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        let table_full_name = FullName::with_schema("public", "table1");
        let table_definition = CreateTable::new(
            &TableName::from(table_full_name.clone()),
            vec![],
            CreateTableMode::Default,
            false,
            &source_span,
        );
        test_context.set_get_table_impl_result(&table_full_name, &Some(table_definition));

        let (_ds, _context) = DataSource::analyze_range_var_node(test_context, &with_clause, syn)?;

        Ok(())
    }

    #[rstest::rstest]
    fn test_analyze_range_var_node_view(mut test_context: TestParseContext) -> TestResult {
        let builder = SynBuilder::new();
        let name = FullName::from_local_name("view1");
        let alias = builder.alias("v1", &[]);
        let syn = builder.range_var(&name, &alias.into());

        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        // CREATE VIEW public.view1 as select 10 as col;
        let table_full_name = FullName::with_schema("public", "view1");
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let mut result_columns = ResultColumnCollection::default();
        let value_expr = SemScalarExpr::new_const(ScalarConstExpr::new_integer(10));
        result_columns.push_column(&ColumnName::from("col"), &value_expr);
        let simple_query =
            SelectStatementNode::new(&result_columns, &with_clause, &from_clause, &None);
        let query = SelectStatement::simple(&simple_query);
        let create_view = CreateView::new(&table_full_name, &query, &[], &None, false);
        let view_def = ViewLike::from(&create_view);
        test_context.set_get_table_impl_result(&table_full_name, &None);
        test_context.set_get_view_impl_result(&table_full_name, &Some(view_def));

        let (_ds, _context) = DataSource::analyze_range_var_node(test_context, &with_clause, syn)?;

        Ok(())
    }
}
