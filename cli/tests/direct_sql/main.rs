//! Direct SQL commands exercise the real authenticated client and server.
#[allow(
    dead_code,
    reason = "shared API fixtures include helpers for other integration targets"
)]
#[path = "../api_helpers.rs"]
mod api_helpers;
mod test_helpers;
use self::{
    api_helpers::{heavy, schema::SchemaFixture},
    test_helpers::Suite,
};

#[tokio::test]
async fn real_query_contracts_and_expected_semantic_errors_pass_without_lockfiles()
-> ::testresult::TestResult {
    // The real service is necessary to check transported diagnostics and result shapes.
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4 NOT NULL);")?;
    let suite = Suite::new(
        &[
            "SELECT id FROM events",
            "INSERT INTO events (id) VALUES (1)",
        ],
        &::serde_json::json!([
            {"diagnostics":[],"columns":[{"name":"id","type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"}]},
            {"diagnostics":[]}
        ]),
    )?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .arg("--junit-xml")
        .arg(suite.report())
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = suite.report_text()?;
    assert!(report.contains("tests=\"2\""));
    assert!(!report.contains("<failure>"));
    Ok(())
}

#[tokio::test]
async fn mixed_results_continue_and_parse_diagnostics_are_assertions() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4 NOT NULL);")?;
    let suite = Suite::new(
        &[
            "SELECT id FROM events",
            "SELECT id FROM events; SELECT id FROM events",
            "SELECT (",
            "INSERT INTO events (id) VALUES (1)",
        ],
        &::serde_json::json!([
            {"diagnostics":["unexpected_expected_code"]}, {"diagnostics":[]}, {"diagnostics":[]}, {"diagnostics":[]}
        ]),
    )?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .arg("--junit-xml")
        .arg(suite.report())
        .output()
        .await?;
    assert_eq!(output.status.code(), Some(1));
    let report = suite.report_text()?;
    assert!(report.contains("tests=\"4\""), "{report}");
    assert!(report.contains("failures=\"3\""), "{report}");
    assert!(report.contains("errors=\"0\""), "{report}");
    assert!(report.contains("expected exactly one statement result, actual 2"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    Ok(())
}

#[rstest::rstest]
#[case::builtin("builtin")]
#[case::default_sql("default")]
#[case::extensions("extensions")]
#[tokio::test]
async fn lower_levels_do_not_require_the_project_dump(
    #[case] level: &str,
) -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("-- not loaded")?;
    std::fs::remove_file(schema.request()["schema_dump"].as_str().unwrap())?;
    let suite = Suite::new(
        &["SELECT 1::int4 AS id"],
        &::serde_json::json!([{"diagnostics":[],"columns":[{"name":"id","type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"}]}]),
    )?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .args(["--schema-level", level, "--expect"])
        .arg(suite.expected())
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!suite.report().exists());
    Ok(())
}

#[tokio::test]
async fn analyze_reports_all_statements_and_continues_after_diagnostics() -> ::testresult::TestResult
{
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4);")?;
    let suite = Suite::new(
        &[
            "SELECT missing FROM events; SELECT id FROM events",
            "SELECT id FROM events",
        ],
        &::serde_json::json!([]),
    )?;
    let output = server
        .sql_command(&schema, "analyze")
        .arg(suite.selection())
        .output()
        .await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing"), "{stderr}");
    assert!(stderr.contains("statement 0"), "{stderr}");
    Ok(())
}

#[tokio::test]
async fn setup_errors_produce_attributed_junit_errors() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("-- setup")?;
    std::fs::remove_file(schema.request()["schema_dump"].as_str().unwrap())?;
    let suite = Suite::new(&["SELECT 1", "SELECT 2"], &::serde_json::json!([[], []]))?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .arg("--junit-xml")
        .arg(suite.report())
        .output()
        .await?;
    assert_eq!(output.status.code(), Some(1));
    let report = suite.report_text()?;
    assert!(report.contains("errors=\"2\""), "{report}");
    assert!(report.contains("schema setup"));
    Ok(())
}

#[tokio::test]
async fn expected_parser_diagnostics_can_pass_and_shape_is_still_checked()
-> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("-- setup")?;
    let suite = Suite::new(
        &["SELECT ("],
        &::serde_json::json!([{"diagnostics":["query_parse","query_parse"]}]),
    )?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[tokio::test]
async fn result_column_contract_materializes_every_page() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4 NOT NULL);")?;
    let suite = Suite::paged_columns()?;
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[tokio::test]
async fn report_write_failure_is_an_execution_error() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("-- setup")?;
    let suite = Suite::new(&["SELECT ("], &::serde_json::json!([["query_parse"]]))?;
    // An existing input file cannot also be a directory for report output.
    let output = server
        .sql_command(&schema, "test")
        .arg(suite.selection())
        .arg("--expect")
        .arg(suite.expected())
        .arg("--junit-xml")
        .arg(suite.expected().join("junit.xml"))
        .output()
        .await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot write JUnit report"));
    Ok(())
}
