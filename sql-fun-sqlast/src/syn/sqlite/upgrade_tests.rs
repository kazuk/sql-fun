//! Parser-to-owned-AST regression cases for the published 0.7.2 grammar.

use super::{Expr, Name, Stmt};
use crate::test_helpers::SqliteSynBuilder;

fn assert_bracket_name(name: &Name, expected: &str) {
    assert_eq!(name.value(), expected);
    // The published parser normalizes bracket-quoted names with Name::exact.
    assert_eq!(*name.quote(), None);
}

#[test]
fn bracket_names_in_statement_positions() {
    for sql in [
        "ALTER TABLE [main].[items] RENAME TO [renamed]",
        "UPDATE [main].[items] SET [value] = 1",
        "ANALYZE [main].[items]",
        "REINDEX [main].[items]",
    ] {
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
