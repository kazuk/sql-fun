//! Public checks for ordered, checked editing of common AST WITH clauses.

// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use ::sql_fun_sqlast::ast::{CteDefinition, Dialect, HaveWithClause, StatementKind};

use crate::test_helpers::common_ast_mutation::{
    cte_names, parse_statement, query_definition, recursive_definition,
    verify_abandoned_empty_editor, verify_with_carrier,
};

#[rstest::rstest]
fn abandoning_empty_postgres_editor_preserves_absent_clause(
    #[values(false, true)] failed_insertion: bool,
    #[values(false, true)] forget_editor: bool,
) {
    verify_abandoned_empty_editor(Dialect::postgres(), failed_insertion, forget_editor);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
fn abandoning_empty_sqlite_editor_preserves_absent_clause(
    #[values(false, true)] failed_insertion: bool,
    #[values(false, true)] forget_editor: bool,
) {
    verify_abandoned_empty_editor(Dialect::sqlite(), failed_insertion, forget_editor);
}

#[test]
fn failed_first_insertion_can_be_retried_with_pending_recursive_choice() {
    let definition = recursive_definition(true, true);
    let mut statement = parse_statement("SELECT 1", Dialect::postgres());
    {
        let mut editor = statement.ensure_with_clause().unwrap();
        editor.set_recursive(true).unwrap();
        assert_eq!(
            editor.insert(1, &definition).unwrap_err().invalid_index(),
            Some(1)
        );
        assert!(editor.view().is_empty());
        assert!(editor.view().is_recursive());
        editor.insert(0, &definition).unwrap();
    }
    let clause = statement
        .with_clause()
        .expect("successful insertion commits the clause");
    assert!(clause.is_recursive());
    assert!(clause.find("traversed").unwrap().has_search());
    assert!(clause.find("traversed").unwrap().has_cycle());
}

#[rstest::rstest]
#[case::select("SELECT 1", StatementKind::query())]
#[case::insert("INSERT INTO items VALUES (1)", StatementKind::insert())]
#[case::update("UPDATE items SET id = 2", StatementKind::update())]
#[case::delete("DELETE FROM items", StatementKind::delete())]
#[case::values("VALUES (1)", StatementKind::query())]
#[case::union("SELECT 1 UNION ALL SELECT 2", StatementKind::query())]
fn edits_postgres_with_carriers(
    #[case] sql: &str,
    #[case] kind: StatementKind,
    #[values(false, true)] existing: bool,
) {
    verify_with_carrier(sql, Dialect::postgres(), kind, existing);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::select("SELECT 1", StatementKind::query())]
#[case::insert("INSERT INTO items VALUES (1)", StatementKind::insert())]
#[case::update("UPDATE items SET id = 2", StatementKind::update())]
#[case::delete("DELETE FROM items", StatementKind::delete())]
#[case::values("VALUES (1)", StatementKind::query())]
#[case::union("SELECT 1 UNION ALL SELECT 2", StatementKind::query())]
fn edits_sqlite_with_carriers(
    #[case] sql: &str,
    #[case] kind: StatementKind,
    #[values(false, true)] existing: bool,
) {
    verify_with_carrier(sql, Dialect::sqlite(), kind, existing);
}

#[test]
fn inserts_at_explicit_positions_without_reordering_existing_ctes() {
    let dialect = Dialect::postgres();
    let mut statement = parse_statement("WITH original AS (SELECT 1) SELECT 1", dialect);
    let mut editor = statement.with_clause_mut().expect("existing WITH");
    editor
        .insert(0, &query_definition("head", dialect))
        .unwrap();
    editor
        .insert(1, &query_definition("middle", dialect))
        .unwrap();
    editor
        .insert(3, &query_definition("tail", dialect))
        .unwrap();
    assert_eq!(
        cte_names(editor.view()),
        ["head", "middle", "original", "tail"]
    );
}

#[rstest::rstest]
#[case::duplicate_name("original", 0)]
#[case::out_of_range("new_cte", 2)]
fn failed_insertions_preserve_order_attributes_and_recursive_choice(
    #[case] name: &str,
    #[case] index: usize,
) {
    let dialect = Dialect::postgres();
    let mut statement = parse_statement(
        "WITH RECURSIVE original(id) AS NOT MATERIALIZED (SELECT 1) SELECT 1",
        dialect,
    );
    let mut editor = statement.with_clause_mut().expect("existing WITH");
    let error = editor
        .insert(index, &query_definition(name, dialect))
        .unwrap_err();
    if index == 0 {
        assert_eq!(error.duplicate_name(), Some(name));
        assert_eq!(error.invalid_index(), None);
    } else {
        assert_eq!(error.invalid_index(), Some(index));
        assert_eq!(error.duplicate_name(), None);
    }
    assert!(!error.is_unsupported_carrier());
    assert_eq!(error.invalid_body_dialect(), None);
    assert!(!error.is_invalid_cte_placement());
    assert!(!error.is_dialect_mismatch());
    assert!(!error.is_invalid_identifier());
    assert!(!error.requires_recursive());
    let view = editor.view();
    assert_eq!(cte_names(view), ["original"]);
    assert!(view.is_recursive());
    let original = view.find("original").expect("original declaration");
    assert_eq!(original.columns().collect::<Vec<_>>(), ["id"]);
    assert_eq!(original.materialized(), Some(false));
}

#[test]
fn unsupported_carriers_reject_with_creation() {
    let mut statement = parse_statement("CREATE TABLE items (id integer)", Dialect::postgres());
    assert!(statement.with_clause().is_none());
    assert!(statement.with_clause_mut().is_none());
    let error = statement
        .ensure_with_clause()
        .err()
        .expect("unsupported carrier");
    assert!(error.is_unsupported_carrier());
    assert_eq!(error.invalid_body_dialect(), None);
    let error = CteDefinition::new("table_cte", &statement).unwrap_err();
    assert_eq!(error.invalid_body_dialect(), Some(Dialect::postgres()));
    assert!(!error.is_unsupported_carrier());
    assert_eq!(statement.kind(), StatementKind::table());
    assert!(statement.with_clause().is_none());
}

#[rstest::rstest]
#[case::insert("INSERT INTO items VALUES (1)", StatementKind::insert())]
#[case::insert_returning("INSERT INTO items VALUES (1) RETURNING id", StatementKind::insert())]
#[case::update("UPDATE items SET id = 2", StatementKind::update())]
#[case::update_returning("UPDATE items SET id = 2 RETURNING id", StatementKind::update())]
#[case::delete("DELETE FROM items", StatementKind::delete())]
#[case::delete_returning("DELETE FROM items RETURNING id", StatementKind::delete())]
fn inserts_postgres_modifying_bodies_under_explicit_recursive_spelling(
    #[case] body_sql: &str,
    #[case] expected: StatementKind,
) {
    let dialect = Dialect::postgres();
    let body = parse_statement(body_sql, dialect);
    let definition = CteDefinition::new("changed", &body).unwrap();
    let mut statement = parse_statement("SELECT 1", dialect);
    let mut editor = statement.ensure_with_clause().unwrap();
    editor.set_recursive(true).unwrap();
    editor.insert(0, &definition).unwrap();
    let view = editor.view();
    assert!(view.is_recursive());
    assert_eq!(view.find("changed").unwrap().body_kind(), expected);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::insert("INSERT INTO items VALUES (1)")]
#[case::insert_returning("INSERT INTO items VALUES (1) RETURNING id")]
#[case::update("UPDATE items SET id = 2")]
#[case::update_returning("UPDATE items SET id = 2 RETURNING id")]
#[case::delete("DELETE FROM items")]
#[case::delete_returning("DELETE FROM items RETURNING id")]
fn rejects_sqlite_modifying_cte_bodies(#[case] body_sql: &str) {
    let body = parse_statement(body_sql, Dialect::sqlite());
    let error = CteDefinition::new("changed", &body).unwrap_err();
    assert_eq!(error.invalid_body_dialect(), Some(Dialect::sqlite()));
}

#[rstest::rstest]
#[case::body_with(
    "WITH changed AS (INSERT INTO items VALUES (1) RETURNING id) SELECT id FROM changed"
)]
#[case::scalar_subquery(
    "SELECT (WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed)"
)]
#[case::from_subquery(
    "SELECT * FROM (WITH changed AS (UPDATE items SET id = 2 RETURNING id) SELECT id FROM changed) nested"
)]
#[case::expression(
    "SELECT CASE WHEN true THEN (WITH changed AS (INSERT INTO items VALUES (1) RETURNING id) SELECT id FROM changed) ELSE 0 END"
)]
#[case::multiassignment(
    "UPDATE items SET (left_id, right_id) = (WITH changed AS (DELETE FROM items RETURNING id) SELECT id, id FROM changed)"
)]
fn rejects_data_modifying_with_anywhere_inside_a_new_cte_body(#[case] body_sql: &str) {
    let body = parse_statement(body_sql, Dialect::postgres());
    let error = CteDefinition::new("outer_cte", &body).unwrap_err();
    assert!(error.is_invalid_cte_placement());
}

#[test]
fn top_level_select_into_cannot_become_a_cte_body() {
    let body = parse_statement("SELECT 1 INTO temp_result", Dialect::postgres());
    assert!(body.is_query());
    let error = CteDefinition::new("outer_cte", &body).unwrap_err();
    assert!(error.is_invalid_cte_placement());
    assert!(body.is_query());
}

#[test]
fn copied_ctes_cannot_bypass_nested_modification_checks() {
    let dialect = Dialect::postgres();
    let source = parse_statement(
        "WITH copied AS (SELECT (WITH changed AS (DELETE FROM items RETURNING id) SELECT id FROM changed)) SELECT 1",
        dialect,
    );
    let definition = source
        .with_clause()
        .unwrap()
        .find("copied")
        .unwrap()
        .to_owned();
    let mut destination = parse_statement("WITH original AS (SELECT 1) SELECT 1", dialect);
    let mut editor = destination.with_clause_mut().unwrap();
    assert!(editor.insert(1, &definition).is_err());
    assert_eq!(cte_names(editor.view()), ["original"]);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::postgres_to_sqlite(Dialect::postgres(), Dialect::sqlite())]
#[case::sqlite_to_postgres(Dialect::sqlite(), Dialect::postgres())]
fn rejects_cross_dialect_definitions_even_for_common_query_syntax(
    #[case] source_dialect: Dialect,
    #[case] target_dialect: Dialect,
) {
    let definition = query_definition("new_cte", source_dialect);
    let mut destination = parse_statement("WITH original AS (SELECT 1) SELECT 1", target_dialect);
    let mut editor = destination.with_clause_mut().unwrap();
    let error = editor.insert(1, &definition).unwrap_err();
    assert!(error.is_dialect_mismatch());
    assert_eq!(error.invalid_body_dialect(), None);
    assert_eq!(cte_names(editor.view()), ["original"]);
}

#[test]
fn new_definitions_preserve_existing_read_only_body_with_clauses() {
    let dialect = Dialect::postgres();
    let body = parse_statement(
        "WITH RECURSIVE inner_cte(id) AS NOT MATERIALIZED (SELECT 1) SELECT id FROM inner_cte",
        dialect,
    );
    let mut definition = CteDefinition::new("outer_cte", &body).unwrap();
    definition.set_columns(&["output"]).unwrap();
    definition.set_materialized(Some(true));
    let mut destination = parse_statement("SELECT 1", dialect);
    destination
        .ensure_with_clause()
        .unwrap()
        .insert(0, &definition)
        .unwrap();
    definition.set_columns(&["changed_alias"]).unwrap();
    definition.set_materialized(None);
    let outer = destination
        .with_clause()
        .unwrap()
        .find("outer_cte")
        .unwrap();
    assert_eq!(outer.columns().collect::<Vec<_>>(), ["output"]);
    assert_eq!(outer.materialized(), Some(true));
    let nested = outer.with_clause().unwrap();
    assert!(nested.is_recursive());
    let inner = nested.find("inner_cte").unwrap();
    assert_eq!(inner.columns().collect::<Vec<_>>(), ["id"]);
    assert_eq!(inner.materialized(), Some(false));
}

#[test]
fn cloned_postgres_ctes_preserve_search_cycle_and_declaration_attributes() {
    let dialect = Dialect::postgres();
    let definition = recursive_definition(true, true);
    let mut destination = parse_statement("SELECT 1", dialect);
    {
        let mut editor = destination.ensure_with_clause().unwrap();
        editor.set_recursive(true).unwrap();
        editor.insert(0, &definition).unwrap();
    }
    let copied = destination
        .with_clause()
        .unwrap()
        .find("traversed")
        .unwrap();
    assert_eq!(copied.columns().collect::<Vec<_>>(), ["id"]);
    assert_eq!(copied.materialized(), Some(true));
    assert!(copied.has_search());
    assert!(copied.has_cycle());
}

#[rstest::rstest]
#[case::search(true, false)]
#[case::cycle(false, true)]
#[case::search_and_cycle(true, true)]
fn search_and_cycle_require_an_explicit_recursive_destination(
    #[case] search: bool,
    #[case] cycle: bool,
) {
    let definition = recursive_definition(search, cycle);
    let mut destination =
        parse_statement("WITH original AS (SELECT 1) SELECT 1", Dialect::postgres());
    let mut editor = destination.with_clause_mut().unwrap();
    let error = editor.insert(1, &definition).unwrap_err();
    assert!(error.requires_recursive());
    assert!(!editor.view().is_recursive());
    assert_eq!(cte_names(editor.view()), ["original"]);
}

#[rstest::rstest]
#[case::search(true, false)]
#[case::cycle(false, true)]
#[case::search_and_cycle(true, true)]
fn search_and_cycle_prevent_disabling_recursive_without_changing_the_clause(
    #[case] search: bool,
    #[case] cycle: bool,
) {
    let definition = recursive_definition(search, cycle);
    let mut destination = parse_statement("SELECT 1", Dialect::postgres());
    let mut editor = destination.ensure_with_clause().unwrap();
    editor.set_recursive(true).unwrap();
    editor.insert(0, &definition).unwrap();
    let error = editor.set_recursive(false).unwrap_err();
    assert!(error.requires_recursive());
    let view = editor.view();
    assert!(view.is_recursive());
    assert_eq!(cte_names(view), ["traversed"]);
    let original = view.find("traversed").unwrap();
    assert_eq!(original.has_search(), search);
    assert_eq!(original.has_cycle(), cycle);
}

#[test]
fn cloned_merge_ctes_preserve_the_existing_body() {
    let dialect = Dialect::postgres();
    let source = parse_statement(
        "WITH merged AS (MERGE INTO items USING source ON true WHEN MATCHED THEN DO NOTHING RETURNING items.id) SELECT 1",
        dialect,
    );
    let definition = source
        .with_clause()
        .unwrap()
        .find("merged")
        .unwrap()
        .to_owned();
    let mut destination = parse_statement("SELECT 1", dialect);
    destination
        .ensure_with_clause()
        .unwrap()
        .insert(0, &definition)
        .unwrap();
    assert_eq!(
        destination
            .with_clause()
            .unwrap()
            .find("merged")
            .unwrap()
            .body_kind(),
        StatementKind::merge(),
    );
}

#[test]
fn postgres_identifier_comparison_preserves_parsed_case_distinctions() {
    let dialect = Dialect::postgres();
    let mut statement = parse_statement("WITH \"Mixed\" AS (SELECT 1) SELECT 1", dialect);
    let mut editor = statement.with_clause_mut().unwrap();
    assert!(editor.view().find("Mixed").is_some());
    assert!(editor.view().find("mixed").is_none());
    editor
        .insert(1, &query_definition("mixed", dialect))
        .unwrap();
    assert!(
        editor
            .insert(2, &query_definition("Mixed", dialect))
            .is_err()
    );
    assert_eq!(cte_names(editor.view()), ["Mixed", "mixed"]);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_identifier_comparison_rejects_ascii_case_duplicates() {
    let dialect = Dialect::sqlite();
    let mut statement = parse_statement("WITH \"Mixed\" AS (SELECT 1) SELECT 1", dialect);
    let mut editor = statement.with_clause_mut().unwrap();
    assert!(editor.view().find("mixed").is_some());
    let error = editor
        .insert(1, &query_definition("MIXED", dialect))
        .unwrap_err();
    assert_eq!(error.duplicate_name(), Some("MIXED"));
    assert_eq!(cte_names(editor.view()), ["Mixed"]);
}

#[rstest::rstest]
#[case::whitespace(" ", true)]
#[case::empty("", false)]
#[case::nul("bad\0name", false)]
fn postgres_definition_names_follow_logical_identifier_rules(
    #[case] name: &str,
    #[case] valid: bool,
) {
    let body = parse_statement("SELECT 1", Dialect::postgres());
    let result = CteDefinition::new(name, &body);
    assert_eq!(result.is_ok(), valid);
    if let Err(error) = result {
        assert!(error.is_invalid_identifier());
    }
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::whitespace(" ", true)]
#[case::empty("", true)]
#[case::nul("bad\0name", false)]
fn sqlite_definition_names_follow_logical_identifier_rules(
    #[case] name: &str,
    #[case] valid: bool,
) {
    let body = parse_statement("SELECT 1", Dialect::sqlite());
    let result = CteDefinition::new(name, &body);
    assert_eq!(result.is_ok(), valid);
    if let Err(error) = result {
        assert!(error.is_invalid_identifier());
    }
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_empty_quoted_names_and_columns_survive_copy_and_insertion() {
    let dialect = Dialect::sqlite();
    let source = parse_statement("WITH \"\"(\"\") AS (SELECT 1) SELECT 1", dialect);
    let definition = source.with_clause().unwrap().find("").unwrap().to_owned();
    let mut destination = parse_statement("SELECT 1", dialect);
    destination
        .ensure_with_clause()
        .unwrap()
        .insert(0, &definition)
        .unwrap();
    let copied = destination.with_clause().unwrap().find("").unwrap();
    assert_eq!(copied.name(), "");
    assert_eq!(copied.columns().collect::<Vec<_>>(), [""]);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_definitions_accept_empty_column_aliases() {
    let body = parse_statement("SELECT 1", Dialect::sqlite());
    let mut definition = CteDefinition::new("", &body).unwrap();
    definition.set_columns(&[""]).unwrap();
    assert_eq!(definition.view().columns().collect::<Vec<_>>(), [""]);
}

#[test]
fn postgres_definitions_reject_empty_column_aliases() {
    let mut definition = query_definition("new_cte", Dialect::postgres());
    let error = definition.set_columns(&[""]).unwrap_err();
    assert!(error.is_invalid_identifier());
    assert_eq!(definition.view().columns().count(), 0);
}

#[test]
fn invalid_column_edits_leave_existing_aliases_unchanged() {
    let mut definition = query_definition("new_cte", Dialect::postgres());
    definition.set_columns(&["original"]).unwrap();
    let error = definition
        .set_columns(&["replacement", "bad\0name"])
        .unwrap_err();
    assert!(error.is_invalid_identifier());
    assert_eq!(
        definition.view().columns().collect::<Vec<_>>(),
        ["original"]
    );
}
