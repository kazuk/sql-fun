use crate::{
    ast::Dialect,
    parse_statements,
    sem::AnalysisError,
    test_helpers::{TestParseContext, create_cache_context},
};

#[rstest::rstest]
#[case::insert("INSERT INTO missing VALUES (1)")]
#[case::update("UPDATE missing SET id = 1")]
#[case::create_rule("CREATE RULE r AS ON INSERT TO missing DO NOTHING")]
#[case::drop_rule("DROP RULE r ON missing")]
#[case::create_trigger("CREATE TRIGGER t BEFORE INSERT ON missing EXECUTE FUNCTION absent()")]
#[case::drop_trigger("DROP TRIGGER t ON missing")]
fn queued_diagnostic_delivery_failure_still_stops_analysis(#[case] sql: &str) {
    let mut cache_context = create_cache_context();
    let mut context = TestParseContext::default();
    context.reject_diagnostics_after(0);
    let statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let error = statement
        .analyze(context, &mut cache_context)
        .err()
        .expect("diagnostic delivery fails");
    assert!(matches!(error, AnalysisError::Io(_)));
    assert_eq!(error.to_string(), "diagnostic sink rejected a problem");
}
