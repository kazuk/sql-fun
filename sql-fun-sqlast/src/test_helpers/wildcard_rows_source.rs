use crate::PostgresDialect;
use crate::ast::{Dialect, FromItem, Query, Statement, StatementNode, TableFunctions};
use crate::names::{AliasName, ColumnName};
use crate::parse_statements;
use crate::sem::{WildcardRowsFunction, WildcardRowsSource};

pub(crate) fn invocation(sql: &str) -> TableFunctions {
    parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .test_rows_functions()
}

impl Statement {
    fn test_rows_functions(self) -> TableFunctions {
        self.into_test_node().test_rows_functions()
    }
}
impl StatementNode {
    fn test_rows_functions(self) -> TableFunctions {
        match self {
            Self::Query(query) => query.test_rows_functions(),
            _ => panic!("query"),
        }
    }
}
pub(crate) fn layout(
    functions: &[&[&str]],
    outputs: &[&str],
    ordinality: bool,
) -> WildcardRowsSource {
    WildcardRowsSource::test_layout(functions, outputs, ordinality)
}
impl WildcardRowsSource {
    fn test_layout(functions: &[&[&str]], outputs: &[&str], ordinality: bool) -> Self {
        let functions = functions
            .iter()
            .map(WildcardRowsFunction::test_layout)
            .collect::<Vec<_>>();
        let outputs = outputs
            .iter()
            .copied()
            .map(ColumnName::from)
            .collect::<Vec<_>>();
        Self::new(&AliasName::from("r"), &functions, &outputs, ordinality)
    }
}
impl WildcardRowsFunction {
    fn test_layout(columns: &&[&str]) -> Self {
        let columns = columns
            .iter()
            .copied()
            .map(ColumnName::from)
            .collect::<Vec<_>>();
        Self::from_columns(&columns)
    }
}

pub(crate) fn rendered(source: FromItem) -> String {
    Query::test_projection_statement(source)
        .deparse_sql(Dialect::postgres())
        .unwrap()
}

/// Verify that rewritten SQL remains accepted by the PostgreSQL parser.
pub(crate) fn assert_parses(sql: &str) {
    PostgresDialect::parse(sql).unwrap();
}
