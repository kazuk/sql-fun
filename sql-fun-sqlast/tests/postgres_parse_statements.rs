//! PostgreSQL parsing through the public statement iterator.

use ::sql_fun_sqlast::ast::Dialect;
use ::sql_fun_sqlast::parse_statements;

#[rstest::rstest]
#[case::unsupported_statement("VACUUM items")]
#[case::unsupported_conversion("DROP VIEW items")]
fn continues_after_statement_conversion_errors(#[case] unsupported: &str) {
    let sql = format!("SELECT 1; {unsupported}; SELECT 2;");
    let mut statements = parse_statements(&sql, Dialect::postgres());
    assert!(statements.next().expect("first query").unwrap().is_query());
    let error = statements.next().expect("conversion error").unwrap_err();
    assert!(!error.is_syntax());
    assert_eq!(
        error.span().get_from(&sql),
        Some(format!(" {unsupported}").as_str())
    );
    assert!(statements.next().expect("last query").unwrap().is_query());
    assert!(statements.next().is_none());
}

#[test]
fn identifies_duplicate_statements_by_their_source_positions() {
    let sql = "VACUUM items;VACUUM items;";
    let mut statements = parse_statements(sql, Dialect::postgres());
    let first = statements.next().expect("first error").unwrap_err();
    let second = statements.next().expect("second error").unwrap_err();
    assert_eq!(first.span().get_from(sql), Some("VACUUM items"));
    assert_eq!(second.span().get_from(sql), Some("VACUUM items"));
    assert_eq!(first.span().end_pos(), "VACUUM items".len());
    assert_eq!(second.span().end_pos(), sql.len() - 1);
    assert!(statements.next().is_none());
}

#[test]
fn reports_byte_ranges_after_multibyte_sql_text() {
    let sql = "SELECT '日本語'; VACUUM items; SELECT 2";
    let mut statements = parse_statements(sql, Dialect::postgres());
    assert!(statements.next().expect("first query").unwrap().is_query());
    let error = statements.next().expect("conversion error").unwrap_err();
    let expected = " VACUUM items";
    assert_eq!(error.span().get_from(sql), Some(expected));
    assert_eq!(
        error.span().end_pos(),
        sql.find(expected).unwrap() + expected.len()
    );
    assert!(statements.next().expect("last query").unwrap().is_query());
    assert!(statements.next().is_none());
}
