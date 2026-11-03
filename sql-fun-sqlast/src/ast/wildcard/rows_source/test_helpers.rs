use crate::{
    StringSpan,
    ast::{Dialect, FromItem, Query, QueryBody, Select, Statement, StatementNode, TableFunctions},
    names::{AliasName, ColumnName},
    parse_statements,
    sem::{WildcardRowsFunction, WildcardRowsSource},
};

pub(super) fn invocation(sql: &str) -> TableFunctions {
    parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .test_rows_functions()
}

impl Statement {
    fn test_rows_functions(self) -> TableFunctions {
        self.node.test_rows_functions()
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
impl Query {
    fn test_rows_functions(self) -> TableFunctions {
        self.body.test_rows_functions()
    }
}
impl QueryBody {
    fn test_rows_functions(self) -> TableFunctions {
        match self {
            Self::Select(select) => select.test_rows_functions(),
            _ => panic!("select"),
        }
    }
}
impl Select {
    fn test_rows_functions(mut self) -> TableFunctions {
        self.from.pop().unwrap().test_rows_functions()
    }
}
impl FromItem {
    fn test_rows_functions(self) -> TableFunctions {
        match self {
            Self::Functions(source) => source,
            _ => panic!("function"),
        }
    }
}

pub(super) fn layout(
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

pub(super) fn rendered(source: FromItem) -> String {
    Statement::from_node(
        StatementNode::query(Query::projection(&[], &[source])),
        Dialect::postgres(),
        StringSpan::default(),
    )
    .deparse_sql(Dialect::postgres())
    .unwrap()
}
