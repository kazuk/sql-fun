use crate::sem::{AnalysisError, FullName, ViewWriteEvaluation};
use crate::test_helpers::TestParseContext;

#[rstest::rstest]
#[case::insert(true)]
#[case::update(false)]
fn direct_evaluation_reports_missing_definition_once(#[case] insert: bool) {
    let mut context = TestParseContext::default();
    let name = FullName::with_schema("public", "missing");
    let error = if insert {
        ViewWriteEvaluation::new(&mut context)
            .insert(&name)
            .unwrap_err()
    } else {
        ViewWriteEvaluation::new(&mut context)
            .update(&name)
            .unwrap_err()
    };
    assert_eq!(
        error.view_problem().unwrap().missing_metadata_view(),
        Some(&name)
    );
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(
        context.reported_problems()[0].code(),
        "view_missing_write_metadata"
    );
    error.recover_view_metadata(&mut context).unwrap();
    assert_eq!(context.reported_problem_count(), 1);
}

#[test]
fn evaluation_retains_original_semantic_failure_and_reports_existing_view() {
    let mut context = TestParseContext::default();
    let name = FullName::with_schema("public", "existing");
    let error = ViewWriteEvaluation::new(&mut context)
        .report::<()>(
            &name,
            Err(AnalysisError::unexpected_input_error(
                "projection analysis failed",
            )),
        )
        .unwrap_err();
    let AnalysisError::ViewEvaluation(cause) = error else {
        panic!("reported evaluation failure")
    };
    assert!(
        matches!(*cause, AnalysisError::UnexpectedInput(ref message) if message == "projection analysis failed")
    );
    assert!(matches!(context.reported_problems()[0].view_problem(),
        Some(crate::sem::problem::ViewProblem::EvaluationFailed(view, _)) if view == &name));
    assert_eq!(
        context.reported_problems()[0].code(),
        "view_evaluation_failed"
    );
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
