//! Direct SQL CLI request contracts and real-server integration tests.
mod test_helpers;

use self::test_helpers::api as api_helpers;

use self::test_helpers::{
    Suite,
    api::{heavy, schema::SchemaFixture},
};

#[rstest::rstest]
#[case::default_warning(vec![], vec!["warning"], true)]
#[case::warning_warning(vec!["--ignore-schema-diagnostics", "warning"], vec!["warning"], true)]
#[case::default_error(vec![], vec!["warning", "error"], false)]
#[case::warning_error(vec!["--ignore-schema-diagnostics", "warning"], vec!["warning", "error"], false)]
#[case::error_error(vec!["--ignore-schema-diagnostics", "error"], vec!["warning", "error"], true)]
#[tokio::test]
async fn schema_diagnostics_remain_visible_and_only_gate_when_requested(
    #[case] args: Vec<&str>,
    #[case] severities: Vec<&str>,
    #[case] continues: bool,
) -> ::testresult::TestResult {
    let mut options = vec!["test", "cases.json"];
    options.extend(args);
    let (output, queries, report) =
        test_helpers::request_peer::run_schema(&options, &severities, "never").await?;
    assert_eq!(output.status.success(), continues);
    assert_eq!(queries.len(), if continues { 2 } else { 0 });
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("schema initialization").count(),
        severities.len(),
        "{stderr}"
    );
    if continues {
        assert!(
            report.contains("tests=\"2\" failures=\"0\" errors=\"0\""),
            "{report}"
        );
    } else {
        assert!(
            report.contains("tests=\"2\" failures=\"0\" errors=\"2\""),
            "{report}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn matching_query_diagnostics_are_quiet_and_failures_precede_context()
-> ::testresult::TestResult {
    let (output, queries, report) = test_helpers::request_peer::run_schema(
        &["test", "diagnostic-cases.json"],
        &["warning"],
        "never",
    )
    .await?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(queries.len(), 4);
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stdout.starts_with(
            "PASS diagnostic-cases.json::expected\nPASS diagnostic-cases.json::expected-error\nFAIL diagnostic-cases.json::unexpected\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("Unexpected:\n      \"parameter_constraint\""),
        "{stdout}"
    );
    assert!(stdout.contains("Case 2, statement 0"), "{stdout}");
    assert!(
        stdout.contains("PASS diagnostic-cases.json::later"),
        "{stdout}"
    );
    assert_eq!(stderr.matches("schema initialization").count(), 1);
    assert!(!stdout.contains('\x1b'));
    assert!(
        report.contains("tests=\"4\" failures=\"1\" errors=\"0\""),
        "{report}"
    );
    assert!(!report.contains('\x1b'));
    Ok(())
}

#[rstest::rstest]
#[case::invalid(vec!["test", "cases.json", "--ignore-schema-diagnostics", "advice"])]
#[case::missing(vec!["test", "cases.json", "--ignore-schema-diagnostics"])]
#[case::analyze(vec!["analyze", "SELECT 1", "--ignore-schema-diagnostics", "error"])]
#[test]
fn schema_threshold_rejects_invalid_options(#[case] args: Vec<&str>) -> ::testresult::TestResult {
    let output = std::process::Command::new(api_helpers::executable::cli_executable())
        .args(args)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    Ok(())
}

#[rstest::rstest]
#[case::auto("auto", false)]
#[case::never("never", false)]
#[case::ansi("ansi", true)]
#[tokio::test]
async fn case_difference_color_obeys_terminal_setting(
    #[case] color: &str,
    #[case] ansi: bool,
) -> ::testresult::TestResult {
    let (output, _, report) =
        test_helpers::request_peer::run_schema(&["test", "diagnostic-cases.json"], &[], color)
            .await?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout.contains(&0x1b), ansi);
    assert!(!report.contains('\x1b'));
    Ok(())
}

#[rstest::rstest]
#[case::missing_context(vec!["error", "no_handle"])]
#[case::fatal_rpc(vec!["fatal"])]
#[tokio::test]
async fn ignored_schema_errors_do_not_override_initialization_failures(
    #[case] severities: Vec<&str>,
) -> ::testresult::TestResult {
    let (output, queries, report) = test_helpers::request_peer::run_schema(
        &["test", "cases.json", "--ignore-schema-diagnostics", "error"],
        &severities,
        "never",
    )
    .await?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(queries.len(), 0);
    assert!(
        report.contains("tests=\"2\" failures=\"0\" errors=\"2\""),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn schema_diagnostics_remain_visible_on_query_execution_error() -> ::testresult::TestResult {
    let (output, queries, report) = test_helpers::request_peer::run_schema(
        &["test", "error-cases.json"],
        &["warning"],
        "never",
    )
    .await?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(queries.len(), 1);
    assert!(String::from_utf8(output.stderr)?.contains("schema initialization"));
    assert!(String::from_utf8(output.stdout)?.starts_with("ERROR error-cases.json::rpc-error"));
    assert!(
        report.contains("tests=\"1\" failures=\"0\" errors=\"1\""),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn multiple_statement_failure_retains_diagnostic_positions() -> ::testresult::TestResult {
    let (output, _, report) =
        test_helpers::request_peer::run_schema(&["test", "multiple-cases.json"], &[], "never")
            .await?;
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.starts_with("FAIL multiple-cases.json::multiple"),
        "{stdout}"
    );
    assert!(
        stdout.contains("expected exactly one statement result, actual 2"),
        "{stdout}"
    );
    assert!(stdout.contains("Case 0, statement 0"), "{stdout}");
    assert!(stdout.contains("Case 0, statement 1"), "{stdout}");
    assert!(
        report.contains("tests=\"1\" failures=\"1\" errors=\"0\""),
        "{report}"
    );
    Ok(())
}

#[rstest::rstest]
#[case::analyze_default(&["analyze", "--sql-file", "queries.json#$.queries"], None, None, "default")]
#[case::test_default(&["test", "cases.json"], None, None, "default")]
#[case::environment(&["analyze", "--sql-file", "queries.json#$.queries"], None, Some("products"), "products")]
#[case::cli_override(&["test", "cases.json"], Some("products"), Some("default"), "products")]
#[tokio::test]
async fn direct_sql_selects_the_requested_datasource(
    #[case] options: &[&str],
    #[case] cli_datasource: Option<&str>,
    #[case] env_datasource: Option<&str>,
    #[case] expected: &str,
) -> ::testresult::TestResult {
    let (output, _, initialization) = test_helpers::request_peer::run_selection(
        options,
        cli_datasource,
        env_datasource,
        Some(true),
    )
    .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let initialization = initialization.expect("schema initialization request");
    assert_eq!(initialization["datasource"], expected);
    assert_eq!(
        initialization["environments"]["SQL_FUN_DATASOURCE"],
        expected
    );
    Ok(())
}

#[rstest::rstest]
#[case::analyze_missing(&["analyze", "--sql-file", "queries.json#$.queries"], None)]
#[case::test_missing(&["test", "cases.json"], None)]
#[case::analyze_invalid(&["analyze", "--sql-file", "queries.json#$.queries"], Some(false))]
#[case::test_invalid(&["test", "cases.json"], Some(false))]
#[tokio::test]
async fn invalid_default_datasource_is_reported(
    #[case] options: &[&str],
    #[case] valid_default: Option<bool>,
) -> ::testresult::TestResult {
    let (output, _, _) =
        test_helpers::request_peer::run_selection(options, None, None, valid_default).await?;
    assert!(!output.status.success());
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.contains("schema setup") && output.contains("default"),
        "{output}"
    );
    Ok(())
}

#[rstest::rstest]
#[case::analyze(vec!["analyze", "--sql-file", "queries.json#$.queries"], true)]
#[case::opt_out(vec!["analyze", "--sql-file", "queries.json#$.queries", "--no-column-access"], false)]
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
    assert!(!stdout.contains("Column access"));
    Ok(())
}

#[rstest::rstest]
#[case::advice("advice case", "OK", 0)]
#[case::warning("warning case", "WARN", 0)]
#[case::mixed("mixed case", "ERROR", 1)]
#[tokio::test]
async fn analyze_status_and_diagnostic_duplication(
    #[case] query: &str,
    #[case] status: &str,
    #[case] exit: i32,
) -> ::testresult::TestResult {
    let (output, _) =
        test_helpers::request_peer::run(&["analyze", query, "--diagnostics-to-stderr"]).await?;
    assert_eq!(output.status.code(), Some(exit));
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stdout.starts_with(&format!("{status} <inline SQL>")),
        "{stdout}"
    );
    assert_eq!(stdout.matches("<inline SQL>").count(), 1);
    assert!(stdout.contains("parameter_constraint"), "{stdout}");
    assert_eq!(
        stdout.matches("parameter_constraint").count(),
        stderr.matches("parameter_constraint").count()
    );
    assert!(!stderr.contains("<inline SQL>"));
    assert!(!stderr.contains("  SQL"));
    assert!(!stdout.contains("Complete"));
    assert_eq!(stdout.contains("Statement 1"), query == "mixed case");
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
        stdout.contains("Diagnostics:\n    Missing:\n      \"unexpected_expected_code\""),
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
        .arg("--sql-file")
        .arg(suite.selection())
        .output()
        .await?;
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OK "), "{stdout}");
    assert!(stdout.contains("ERROR "), "{stdout}");
    assert!(stdout.contains("Statement 0"), "{stdout}");
    assert!(stdout.contains("Statement 1"), "{stdout}");
    assert!(stdout.contains("missing"), "{stdout}");
    assert!(!stdout.contains("Complete:"), "{stdout}");
    assert!(!stdout.contains("Query kind:"), "{stdout}");
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
        .arg("--sql-file")
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
        stdout.contains(&format!("OK {}[0]", suite.selection())),
        "{stdout}"
    );
    assert!(stdout.contains("Statement 0"), "{stdout}");
    assert!(stdout.contains("Statement 1"), "{stdout}");
    assert!(stdout.contains("  SQL"), "{stdout}");
    assert!(stdout.contains("Parameters"), "{stdout}");
    assert!(stdout.contains("$1"), "{stdout}");
    assert!(stdout.contains("pg_catalog.int4"), "{stdout}");
    assert!(stdout.contains("Result columns"), "{stdout}");
    assert!(stdout.contains("Column access"), "{stdout}");
    assert!(stdout.contains("public.events.id"), "{stdout}");
    assert!(stdout.contains("SelectOutput"), "{stdout}");
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
        .arg("--sql-file")
        .arg(suite.selection())
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout.contains("  SQL\n    unavailable"), "{stdout}");
    assert!(!stdout.contains("Parameters"), "{stdout}");
    assert!(!stdout.contains("Column access"), "{stdout}");
    assert!(stdout.contains("query_parse"), "{stdout}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("query_parse"));
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
        stdout.contains("Column 0 · name\n    Expected: \"renamed\"\n    Actual:   \"id\""),
        "{stdout}"
    );
    assert!(stdout.contains("Column 0 · type\n    Expected: \"app\".\"identifier\"[]\n    Actual:   \"pg_catalog\".\"int4\""), "{stdout}");
    assert!(
        stdout.contains("Column 0 · nullability\n    Expected: nullable\n    Actual:   not_null"),
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
        failure.contains("Column 0 · name\n  Expected:"),
        "{failure}"
    );
    assert!(failure.contains("\n  Actual:"), "{failure}");
    Ok(())
}
