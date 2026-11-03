use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, Statement, WithClause};
use ::sql_fun_sqlast::parse_statements;

pub(crate) fn parse_statement(sql: &str, dialect: Dialect) -> Statement {
    let mut statements = parse_statements(sql, dialect);
    let statement = statements.next().expect("one SQL statement").unwrap();
    assert!(statements.next().is_none(), "single-statement fixture");
    statement
}

pub(crate) fn query_definition(name: &str, dialect: Dialect) -> CteDefinition {
    CteDefinition::new(name, &parse_statement("SELECT 1", dialect)).unwrap()
}

pub(crate) fn recursive_definition(search: bool, cycle: bool) -> CteDefinition {
    let search = if search {
        " SEARCH DEPTH FIRST BY id SET seq"
    } else {
        ""
    };
    let cycle = if cycle {
        " CYCLE id SET cycle USING path"
    } else {
        ""
    };
    let sql = format!(
        "WITH RECURSIVE traversed(id) AS MATERIALIZED (SELECT 1 UNION ALL SELECT id + 1 FROM traversed WHERE id < 2){search}{cycle} SELECT id FROM traversed",
    );
    let statement = parse_statement(&sql, Dialect::postgres());
    statement
        .with_clause()
        .unwrap()
        .find("traversed")
        .unwrap()
        .to_owned()
}

pub(crate) fn cte_names(clause: WithClause<'_>) -> Vec<String> {
    clause.iter().map(|cte| cte.name().to_owned()).collect()
}
