use crate::test_helpers::TestParseContext;
use crate::{
    names::FullName,
    sem::{AnalysisError, ResolutionFailure, ViewWriteEvaluation},
};

#[rstest::rstest]
#[case::insert(true)]
#[case::update(false)]
fn direct_evaluation_reports_missing_definition_once(#[case] insert: bool) {
    let mut context = TestParseContext::default();
    let name = FullName::with_schema("public", "missing");
    let unresolved = if insert {
        ViewWriteEvaluation::new(&mut context)
            .insert(&name)
            .unwrap()
            .is_none()
    } else {
        ViewWriteEvaluation::new(&mut context)
            .update(&name)
            .unwrap()
            .is_none()
    };
    assert!(unresolved);
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(
        context.reported_problems()[0]
            .view_problem()
            .unwrap()
            .missing_metadata_view()
            .map(|view| view.as_str()),
        Some(name.to_string().as_str())
    );
}

#[test]
fn internal_contract_failure_is_propagated_without_a_semantic_problem() {
    let mut context = TestParseContext::default();
    let result: Result<(), ResolutionFailure> =
        Err(AnalysisError::unexpected_input_error("projection analysis contract failed").into());
    let error = ResolutionFailure::finish(result, &mut context).unwrap_err();
    assert!(
        matches!(error, AnalysisError::UnexpectedInput(ref message) if message == "projection analysis contract failed")
    );
    assert_eq!(context.reported_problem_count(), 0);
}

#[test]
fn diagnostic_delivery_failure_is_propagated() {
    let mut context = TestParseContext::default();
    context.reject_diagnostics_after(0);
    let error = ViewWriteEvaluation::new(&mut context)
        .insert(&FullName::with_schema("public", "missing"))
        .unwrap_err();
    assert!(matches!(error, AnalysisError::Io(_)));
    assert_eq!(error.to_string(), "diagnostic sink rejected a problem");
    assert_eq!(context.reported_problem_count(), 0);
}

#[rstest::rstest]
#[case::insert("INSERT INTO missing VALUES (1)")]
#[case::update("UPDATE missing SET id = 1")]
#[case::create_rule("CREATE RULE r AS ON INSERT TO missing DO NOTHING")]
#[case::drop_rule("DROP RULE r ON missing")]
#[case::create_trigger("CREATE TRIGGER t BEFORE INSERT ON missing EXECUTE FUNCTION absent()")]
#[case::drop_trigger("DROP TRIGGER t ON missing")]
fn queued_diagnostic_delivery_failure_still_stops_analysis(#[case] sql: &str) {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let mut context = TestParseContext::default();
    context.reject_diagnostics_after(0);
    let statement = crate::parse_statements(sql, crate::ast::Dialect::postgres())
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
