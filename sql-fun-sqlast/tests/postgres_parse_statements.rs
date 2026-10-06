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
    assert!(error.is_recoverable());
    assert!(!error.is_boundary_discovery());
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

#[rstest::rstest]
#[case::syntax("SELECT FROM")]
#[case::skipped_identifier("asdf")]
#[case::skipped_operator("+")]
#[case::multibyte("\"日本語\"")]
#[case::eof_like("SELECT 1 +")]
#[case::closed_body("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT FROM; SELECT 2; END")]
#[case::misplaced_end("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT END; SELECT 2; END")]
#[case::unclosed_case("SELECT CASE WHEN true THEN 1")]
#[case::unclosed_case_in_body(
    "CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT CASE WHEN true THEN 1; END"
)]
fn continues_after_recoverable_syntax_errors(#[case] malformed: &str) {
    let prefix = "SELECT '日本語';";
    let sql = format!("{prefix} {malformed}; SELECT 3;");
    let mut statements = parse_statements(&sql, Dialect::postgres());
    assert!(
        statements
            .next()
            .expect("preceding query")
            .unwrap()
            .is_query()
    );
    let error = statements.next().expect("malformed range").unwrap_err();
    assert!(error.is_syntax());
    assert!(error.is_recoverable());
    assert!(!error.is_boundary_discovery());
    assert_eq!(
        error.span().get_from(&sql),
        Some(format!(" {malformed}").as_str())
    );
    assert_eq!(
        error.span().seek_pos(),
        std::io::SeekFrom::Start(prefix.len() as u64)
    );
    assert!(error.position().is_none());
    assert!(
        statements
            .next()
            .expect("following query")
            .unwrap()
            .is_query()
    );
    assert!(statements.next().is_none());
    assert!(statements.next().is_none());
}

#[test]
fn reports_every_skipped_range_including_leading_and_trailing_input() {
    let sql = "asdf;/* ; */+;;SELECT 1;\"日本語\";asdf";
    let mut statements = parse_statements(sql, Dialect::postgres());
    for range in ["asdf", "/* ; */+"] {
        let error = statements
            .next()
            .expect("leading invalid range")
            .unwrap_err();
        assert_eq!(error.span().get_from(sql), Some(range));
        assert!(error.is_recoverable());
    }
    assert!(statements.next().expect("query").unwrap().is_query());
    let error = statements
        .next()
        .expect("multibyte invalid range")
        .unwrap_err();
    assert_eq!(error.span().get_from(sql), Some("\"日本語\""));
    let error = statements
        .next()
        .expect("trailing invalid range")
        .unwrap_err();
    assert_eq!(error.span().get_from(sql), Some("asdf"));
    assert_eq!(error.span().end_pos(), sql.len());
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::literal("SELECT 'semi;colon'")]
#[case::escaped_literal("SELECT E'escaped\\\';literal'")]
#[case::unicode_literal("SELECT U&'semi;colon'")]
#[case::unicode_identifier("SELECT U&\"semi;colon\"")]
#[case::identifier("SELECT \"semi;colon\"")]
#[case::dollar_quote("SELECT $tag$semi;colon$tag$")]
#[case::nested_comments("SELECT 1 /* outer ; /* inner ; */ still outer ; */")]
#[case::line_comment("SELECT 1 -- ;\n")]
fn quoted_separators_remain_inside_statements_during_recovery(#[case] query: &str) {
    let sql = format!("asdf;{query};SELECT 3");
    let mut statements = parse_statements(&sql, Dialect::postgres());
    assert!(statements.next().unwrap().unwrap_err().is_recoverable());
    assert!(statements.next().expect("quoted query").unwrap().is_query());
    assert!(
        statements
            .next()
            .expect("following query")
            .unwrap()
            .is_query()
    );
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::function(
    "CREATE FUNCTION f() RETURNS int LANGUAGE SQL BEGIN ATOMIC SELECT 1; SELECT 2; END"
)]
#[case::procedure("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT 1; END")]
#[case::rule("CREATE RULE r AS ON INSERT TO t DO (SELECT 1; SELECT 2;)")]
fn legal_compound_is_never_reported_as_a_syntax_failure(#[case] compound: &str) {
    let sql = format!("asdf;{compound};SELECT 3");
    let mut statements = parse_statements(&sql, Dialect::postgres());
    assert!(statements.next().unwrap().unwrap_err().is_recoverable());
    if let Err(error) = statements.next().expect("one complete compound statement") {
        assert!(!error.is_syntax(), "{}", error.message());
        assert!(error.is_recoverable());
        assert_eq!(error.span().get_from(&sql), Some(compound));
    }
    assert!(
        statements
            .next()
            .expect("following query")
            .unwrap()
            .is_query()
    );
    assert!(statements.next().is_none());
}

#[rstest::rstest]
#[case::string("SELECT 1; SELECT 'unfinished; SELECT 3;")]
#[case::dollar_quote("SELECT 1; SELECT $tag$unfinished; SELECT 3;")]
#[case::identifier("SELECT 1; SELECT \"unfinished; SELECT 3;")]
#[case::comment("SELECT 1; /* unfinished; SELECT 3;")]
#[case::nested_comment("SELECT 1; /* outer /* inner */ unfinished;")]
#[case::nul("SELECT 1; SELECT '\0'; SELECT 3;")]
fn lexical_failure_yields_one_terminal_error(#[case] sql: &str) {
    let mut statements = parse_statements(sql, Dialect::postgres());
    let error = statements.next().expect("terminal error").unwrap_err();
    assert!(error.is_syntax());
    assert!(error.is_boundary_discovery());
    assert!(!error.is_recoverable());
    assert_eq!(error.span().get_from(sql), Some(sql));
    assert!(statements.next().is_none());
    assert!(statements.next().is_none());
}

#[test]
fn terminal_structural_failure_follows_established_statements() {
    let sql = "SELECT 1; CREATE PROCEDURE p() BEGIN ATOMIC SELECT 2; SELECT 3;";
    let mut statements = parse_statements(sql, Dialect::postgres());
    assert!(
        statements
            .next()
            .expect("established query")
            .unwrap()
            .is_query()
    );
    let error = statements.next().expect("unresolved tail").unwrap_err();
    assert!(error.is_boundary_discovery());
    assert!(!error.is_recoverable());
    assert_eq!(
        error.span().get_from(sql),
        Some(" CREATE PROCEDURE p() BEGIN ATOMIC SELECT 2; SELECT 3;")
    );
    assert!(statements.next().is_none());
    assert!(statements.next().is_none());
}

#[test]
fn repeated_malformed_ranges_have_distinct_byte_positions() {
    let sql = "SELECT '日本語';asdf;asdf;SELECT 3";
    let mut statements = parse_statements(sql, Dialect::postgres());
    assert!(statements.next().unwrap().is_ok());
    let first = statements.next().unwrap().unwrap_err();
    let second = statements.next().unwrap().unwrap_err();
    assert_eq!(first.span().get_from(sql), Some("asdf"));
    assert_eq!(second.span().get_from(sql), Some("asdf"));
    assert_eq!(
        second.span().end_pos() - first.span().end_pos(),
        "asdf;".len()
    );
    assert!(statements.next().unwrap().is_ok());
    assert!(statements.next().is_none());
}

#[test]
fn collecting_results_preserves_errors_or_stops_by_caller_choice() {
    let sql = "SELECT 1;asdf;SELECT 3";
    let results: Vec<_> = parse_statements(sql, Dialect::postgres()).collect();
    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].as_ref().unwrap_err().is_recoverable());
    assert!(results[2].is_ok());
    let error = parse_statements(sql, Dialect::postgres())
        .collect::<Result<Vec<_>, _>>()
        .unwrap_err();
    assert_eq!(error.span().get_from(sql), Some("asdf"));
}

#[cfg(not(feature = "sqlite"))]
#[test]
fn unavailable_dialect_is_terminal_without_claiming_a_syntax_failure() {
    let mut statements = parse_statements("SELECT 1", Dialect::sqlite());
    let error = statements.next().expect("unavailable dialect").unwrap_err();
    assert!(!error.is_syntax());
    assert!(!error.is_boundary_discovery());
    assert!(!error.is_recoverable());
    assert!(statements.next().is_none());
}
