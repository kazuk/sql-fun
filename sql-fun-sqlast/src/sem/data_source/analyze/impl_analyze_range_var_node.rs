use crate::{
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, FullName, ParseContext, TableName, ViewName,
        WithClause,
        data_source::{CteDataSource, TableDataSource, ViewDataSource},
    },
    syn::Opt,
};

use super::DataSource;

impl DataSource {
    pub(super) fn analyze_range_var_node<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        range_var: crate::syn::RangeVar,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let rel_fullname = FullName::try_from(range_var.clone())?;
        let relname = rel_fullname.to_string();
        let alias_name = if let Some(alias) = range_var.get_alias().as_inner() {
            let (alias, new_context) = AliasSpec::analyze(context, &rel_fullname, &alias)?;
            context = new_context;
            alias
        } else {
            AliasSpec::from_local_name(&rel_fullname.local_name())
        };

        if let Some(_cte) = with_clause.get(&relname) {
            Ok((
                CteDataSource::new_data_source(&relname, &alias_name),
                context,
            ))
        } else if let Some(_table) = context.get_table(&TableName::try_from(range_var.clone())?) {
            Ok((
                TableDataSource::new_data_source(&TableName::try_from(range_var)?, &alias_name),
                context,
            ))
        } else if let Some(_view) = context.get_view(&ViewName::from(range_var.clone())) {
            Ok((
                ViewDataSource::new_data_source(&range_var, &alias_name),
                context,
            ))
        } else {
            context.report_problem(AnalysisProblem::relation_not_found(&TableName::try_from(
                range_var,
            )?))?;
            Ok((Self::unknown(&relname, &alias_name), context))
        }
    }
}

#[cfg(test)]
mod test_data_source_analyze_range_var_node {
    use testresult::TestResult;

    use crate::{
        sem::{
            ColumnName, CreateTable, CreateView, FromClause, FullName, ScalarConstExpr, SchemaName,
            SelectStatement, SemScalarExpr, SourcePathWithSpan, TableName, ViewLike, WithClause,
            select_statement::{ResultColumnCollection, SelectStatementNode},
        },
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };

    use super::DataSource;

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

        test_context.set_get_search_path_result(&vec![
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        let table_full_name = FullName::with_schema("public", "table1");
        let table_definition = CreateTable::new(
            &TableName::from(table_full_name.clone()),
            vec![],
            crate::sem::CreateTableMode::Default,
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

        test_context.set_get_search_path_result(&vec![
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
        let create_view = CreateView::new(&table_full_name, &query, &vec![], &None, false);
        let view_def = ViewLike::from(&create_view);
        test_context.set_get_table_impl_result(&table_full_name, &None);
        test_context.set_get_view_impl_result(&table_full_name, &Some(view_def));

        let (_ds, _context) = DataSource::analyze_range_var_node(test_context, &with_clause, syn)?;

        Ok(())
    }
}
