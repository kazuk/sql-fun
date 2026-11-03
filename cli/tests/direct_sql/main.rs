//! Direct SQL CLI request contracts and real-server integration tests.
mod test_helpers;

use self::test_helpers::api as api_helpers;

use self::test_helpers::{
    Suite,
    api::{heavy, schema::SchemaFixture},
};

#[rstest::rstest]
#[case::analyze(vec!["analyze", "queries.json#$.queries"], true)]
#[case::opt_out(vec!["analyze", "queries.json#$.queries", "--no-column-access"], false)]
#[case::named_test(vec!["test", "cases.json"], false)]
#[case::legacy_test(vec!["test", "queries.json#$.queries", "--expect", "queries.json#$.expected"], false)]
#[tokio::test]
async fn command_requests_access_only_for_default_analyze(
    #[case] options: Vec<&str>,
    #[case] access: bool,
) -> ::testresult::TestResult {
    let (output, requests) = test_helpers::request_peer::run(&options).await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request["query"]["Text"], format!("SELECT {}", index + 1));
        assert_eq!(
            request.get("analyze_access"),
            if access {
                Some(&::serde_json::Value::Bool(true))
            } else {
                None
            }
        );
    }
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(
        stdout.matches("Column access (0):").count(),
        if access { 2 } else { 0 }
    );
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
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
#[ignore = "High CI disk usage from server builds"]
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 passed"));
    assert!(
        stdout.contains("diagnostics:\n    missing:\n      \"unexpected_expected_code\""),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("\n\nFAIL {}[1]", suite.selection())),
        "{stdout}"
    );
    Ok(())
}

#[rstest::rstest]
#[case::builtin("builtin")]
#[case::default_sql("default")]
#[case::extensions("extensions")]
#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
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
#[ignore = "High CI disk usage from server builds"]
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PASS"), "{stdout}");
    assert_eq!(stdout.matches("Query kind: Select").count(), 3, "{stdout}");
    assert!(stdout.contains("statement 1"), "{stdout}");
    assert!(stdout.contains("Complete: false"), "{stdout}");
    assert!(stdout.contains("Complete: true"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing"), "{stderr}");
    assert!(stderr.contains("statement 0"), "{stderr}");
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
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
#[ignore = "High CI disk usage from server builds"]
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
#[ignore = "High CI disk usage from server builds"]
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
#[ignore = "High CI disk usage from server builds"]
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

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn analyze_displays_statement_results_and_accesses() -> ::testresult::TestResult {
    // A real service verifies that completed API collections reach the console.
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4 NOT NULL);")?;
    let suite = Suite::new(
        &["SELECT id, ${id:int4} AS argument FROM events; INSERT INTO events (id) VALUES (1)"],
        &::serde_json::json!([]),
    )?;
    let output = server
        .sql_command(&schema, "analyze")
        .arg(suite.selection())
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains(&format!("{}[0] (statement 0)", suite.selection())),
        "{stdout}"
    );
    assert!(stdout.contains("statement 1"), "{stdout}");
    assert!(stdout.contains("Query kind: Select"), "{stdout}");
    assert!(stdout.contains("Query kind: Insert"), "{stdout}");
    assert!(stdout.contains("Complete: true"), "{stdout}");
    assert!(stdout.contains("Executable SQL:"), "{stdout}");
    assert!(stdout.contains("$1"), "{stdout}");
    assert!(stdout.contains("Parameters (1):"), "{stdout}");
    assert!(stdout.contains("$1 id: pg_catalog.int4;"), "{stdout}");
    assert!(
        stdout.contains("[0] id: pg_catalog.int4; not_null"),
        "{stdout}"
    );
    assert!(stdout.contains("Result columns (0):"), "{stdout}");
    assert!(stdout.contains("public.events (Table)"), "{stdout}");
    assert!(
        stdout.contains("public.events.id (Table): SelectOutput"),
        "{stdout}"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn analyze_displays_unavailable_results_for_parser_diagnostics() -> ::testresult::TestResult {
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("-- setup")?;
    let suite = Suite::new(&["SELECT ("], &::serde_json::json!([]))?;
    let output = server
        .sql_command(&schema, "analyze")
        .arg(suite.selection())
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout.contains("Query kind: unavailable"), "{stdout}");
    assert!(stdout.contains("Complete: false"), "{stdout}");
    assert!(
        stdout.contains("Executable SQL:\n    unavailable"),
        "{stdout}"
    );
    assert!(stdout.contains("Parameters (0):"), "{stdout}");
    assert!(stdout.contains("Column access (0):"), "{stdout}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("query_parse"));
    Ok(())
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn column_differences_are_readable_in_console_and_junit() -> ::testresult::TestResult {
    // A real CLI run must deliver the same readable differences to both outputs.
    let server = heavy::ApiServer::new()?;
    let schema = SchemaFixture::with_query_schema("CREATE TABLE events (id int4 NOT NULL);")?;
    let suite = Suite::new(
        &["SELECT id FROM events"],
        &::serde_json::json!([
            {"diagnostics":[],"columns":[{"name":"renamed","type":{"schema_name":"app","local_name":"identifier","is_array":true},"nullability":"nullable"}]}
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("column 0:\n    expected:\n      name: \"renamed\""),
        "{stdout}"
    );
    assert!(
        stdout.contains("type: \"app\".\"identifier\"[]\n      nullability: nullable"),
        "{stdout}"
    );
    assert!(stdout.contains("actual:\n      name: \"id\""), "{stdout}");
    assert!(
        stdout.contains("type: \"pg_catalog\".\"int4\"\n      nullability: not_null"),
        "{stdout}"
    );
    let report = suite.report_text()?;
    let mut reader = ::quick_xml::Reader::from_str(&report);
    let failure = loop {
        match reader.read_event()? {
            ::quick_xml::events::Event::Start(element) if element.name().as_ref() == "failure" => {
                break reader
                    .read_text(element.name())?
                    .xml_content(::quick_xml::XmlVersion::Implicit1_0)
                    .into_owned();
            }
            ::quick_xml::events::Event::Eof => panic!("missing failure in {report}"),
            _ => {}
        }
    };
    assert!(
        failure.contains("column 0:\n  expected:\n    name:"),
        "{failure}"
    );
    assert!(failure.contains("\n  actual:\n    name:"), "{failure}");
    Ok(())
}
