use ::sql_fun_sqlast::ast::{
    CteDefinition, Dialect, HaveWithClause, Statement, StatementKind, WithClause,
};
use ::sql_fun_sqlast::parse_statements;

pub(crate) fn verify_abandoned_empty_editor(
    dialect: Dialect,
    failed_insertion: bool,
    forget_editor: bool,
) {
    let mut statement = parse_statement("SELECT 1", dialect);
    let definition = query_definition("source", dialect);
    let mut editor = statement.ensure_with_clause().unwrap();
    editor.set_recursive(true).unwrap();
    if failed_insertion {
        assert_eq!(
            editor.insert(1, &definition).unwrap_err().invalid_index(),
            Some(1)
        );
    }
    assert!(editor.view().is_empty());
    assert!(editor.view().is_recursive());
    if forget_editor {
        std::mem::forget(editor);
    } else {
        drop(editor);
    }
    assert!(statement.with_clause().is_none());
    assert!(statement.with_clause_mut().is_none());
    {
        let mut editor = statement.ensure_with_clause().unwrap();
        assert!(
            !editor.view().is_recursive(),
            "abandoned edits must not survive"
        );
        editor.insert(0, &definition).unwrap();
    }
    assert!(!statement.with_clause().unwrap().is_recursive());
}

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

pub(crate) fn verify_with_carrier(
    sql: &str,
    dialect: Dialect,
    expected_kind: StatementKind,
    existing: bool,
) {
    let sql = if existing {
        format!("WITH RECURSIVE existing AS (SELECT 1) {sql}")
    } else {
        sql.to_owned()
    };
    let mut statement = parse_statement(&sql, dialect);
    assert_eq!(statement.kind(), expected_kind);
    assert_eq!(statement.with_clause().is_some(), existing);
    assert_eq!(statement.with_clause_mut().is_some(), existing);
    {
        let mut editor = statement.ensure_with_clause().unwrap();
        assert_eq!(editor.view().is_recursive(), existing);
        editor
            .insert(0, &query_definition("new_cte", dialect))
            .unwrap();
        let expected = if existing {
            vec!["new_cte", "existing"]
        } else {
            vec!["new_cte"]
        };
        assert_eq!(cte_names(editor.view()), expected);
        editor.set_recursive(false).unwrap();
        assert!(!editor.view().is_recursive());
        editor.set_recursive(true).unwrap();
    }
    assert!(
        statement
            .ensure_with_clause()
            .unwrap()
            .view()
            .is_recursive()
    );
    assert!(statement.with_clause().unwrap().is_recursive());
}
