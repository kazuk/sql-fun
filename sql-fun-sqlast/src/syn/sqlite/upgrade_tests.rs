//! Parser-to-owned-AST regression cases for the published 0.8.1 grammar.

use super::{Expr, Name, Stmt};
use crate::test_helpers::{SqliteArrayTypeGate, SqliteSynBuilder};

fn assert_bracket_name(name: &Name, expected: &str) {
    assert_eq!(name.value(), expected);
    assert_eq!(*name.quote(), Some('['));
}

#[rstest::rstest]
#[case::alter("ALTER TABLE [main].[items] RENAME TO [renamed]")]
#[case::update("UPDATE [main].[items] SET [value] = 1")]
#[case::analyze("ANALYZE [main].[items]")]
#[case::reindex("REINDEX [main].[items]")]
fn bracket_names_in_statement_positions(#[case] sql: &str) {
    let statement = SqliteSynBuilder.parse_as_stmt(sql);
    let name = match &statement {
        Stmt::AlterTable(alter) => alter.name(),
        Stmt::Update(update) => {
            assert_bracket_name(&update.sets()[0].col_names()[0], "value");
            update.table_name()
        }
        Stmt::Analyze(analyze) => analyze.name().as_ref().unwrap(),
        Stmt::Reindex(reindex) => reindex.idx_name().as_ref().unwrap(),
        _ => panic!("unexpected statement: {sql}"),
    };
    assert_bracket_name(name.name(), "items");
    assert_bracket_name(name.db_name().unwrap(), "main");
}

#[test]
fn bracket_database_name_in_vacuum() {
    let Stmt::Vacuum(vacuum) = SqliteSynBuilder.parse_as_stmt("VACUUM [main] INTO 'copy.db'")
    else {
        panic!("expected VACUUM");
    };
    assert_bracket_name(vacuum.db_name().as_ref().unwrap(), "main");
    assert!(super::VacuumStatement::into(&vacuum).is_some());
}

#[test]
fn bracket_names_in_qualified_expressions_and_types() {
    let Expr::DoublyQualified(qualified) =
        SqliteSynBuilder.parse_expression("SELECT [main].[items].[value]")
    else {
        panic!("expected qualified expression");
    };
    assert_bracket_name(qualified.schema(), "main");
    assert_bracket_name(qualified.table(), "items");
    assert_bracket_name(qualified.column(), "value");
    let statement = SqliteSynBuilder.parse_as_stmt("CREATE TABLE [items] ([value] [custom_type])");
    let table = statement.as_create_table().unwrap();
    let columns = table.body().as_columns_and_constraint().unwrap();
    assert_bracket_name(columns.columns()[0].column_name(), "value");
    assert_eq!(
        columns.columns()[0].column_type().unwrap().name(),
        "custom_type"
    );
}

#[rstest::rstest]
fn parenthesized_subqueries_retain_membership_and_negation(
    #[values("(SELECT 2)", "((SELECT 2))", "(((SELECT 2)))")] rhs: &str,
    #[values(("IN", false), ("NOT IN", true))] operator_case: (&str, bool),
) {
    let (op, negated) = operator_case;
    let expr = SqliteSynBuilder.parse_expression(&format!("SELECT 1 {op} {rhs}"));
    let membership = expr.as_in_select().expect("subquery membership");
    assert_eq!(*membership.not(), negated);
    assert_eq!(membership.lhs().numeric_literal_text(), Some("1"));
    assert!(membership.rhs().with().is_none());
    assert!(expr.as_in_list().is_none());
}

#[test]
fn ordinary_in_list_retains_list_classification() {
    let expr = SqliteSynBuilder.parse_expression("SELECT 1 IN (2, 3)");
    assert_eq!(expr.as_in_list().unwrap().rhs().len(), 2);
    assert!(expr.as_in_select().is_none());
}

#[test]
fn cast_retains_array_dimensions() {
    let expr = SqliteSynBuilder.parse_expression("SELECT CAST(1 AS TEXT[][])");
    let cast = expr.as_cast().unwrap();
    let ty = cast.type_name().as_ref().unwrap();
    assert_eq!(ty.name(), "TEXT");
    assert_eq!(
        ty.array_dimensions(&mut SqliteArrayTypeGate::default())
            .unwrap(),
        2
    );
    assert_eq!(cast.expr().numeric_literal_text(), Some("1"));
}

#[test]
fn delete_retains_where_and_returning() {
    let stmt = SqliteSynBuilder.parse_as_stmt(
        "WITH source AS (SELECT 1) DELETE FROM main.items NOT INDEXED WHERE id = 1 RETURNING id, value AS old_value",
    );
    let delete = stmt.as_delete().unwrap();
    assert_eq!(delete.with().as_ref().unwrap().ctes().len(), 1);
    assert_eq!(delete.table_name().name().value(), "items");
    assert_eq!(delete.table_name().db_name().unwrap().value(), "main");
    assert!(delete.indexed().is_some());
    assert!(delete.where_clause().is_some());
    assert_eq!(delete.returning().len(), 2);
}
