use crate::{
    analysis::PreparedDiagnostics,
    check::submission::{submit, test_helpers, test_helpers::collect_submission},
};

const QUERY: &str = "SELECT ${shared:int4}, id FROM a_visible";

#[tokio::test]
async fn submits_recovered_catalogs_with_datasource_scoped_contexts() {
    let project = test_helpers::Project::new();
    let catalog = test_helpers::catalog();
    let expected = ::serde_json::to_value(&catalog).unwrap();
    let expected_fingerprint = catalog
        .finger_print(&::sql_fun_core::PlaceholderedSql::from_query(QUERY))
        .unwrap();
    let expected_query = QUERY.to_owned();
    let (mut client, peer) = test_helpers::peer(None).await;
    let analyses = collect_submission(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first", "second", "first"], QUERY, &catalog),
    )
    .await
    .unwrap();
    client.finish().await.unwrap();
    let requests = peer.await.unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(analyses.len(), 3);
    assert_eq!(analyses[2].function_name, "query_2");
    assert_eq!(analyses[2].source_file, "src/queries.rs");
    assert_eq!(analyses[2].source_line, 42);
    assert_eq!(analyses[2].datasource, "first");
    assert_eq!(analyses[2].original_query, expected_query);
    assert_eq!(analyses[2].query_finger_print, expected_fingerprint);
    for (index, datasource) in [(1, "first"), (3, "second"), (4, "first")] {
        let request = &requests[index];
        assert_eq!(request["method"], "analyze_query");
        assert_eq!(request["params"]["analyze_access"], true);
        assert_eq!(
            request["params"]["context_handle"],
            format!("context-{datasource}")
        );
        assert_eq!(request["params"]["query"]["Text"], QUERY);
        assert!(
            request["params"]["resolved_cte_catalog"]
                .get("base_query")
                .is_none()
        );
        assert_eq!(request["params"]["resolved_cte_catalog"], expected);
        let restored: ::sql_fun_core::ResolvedCteCatalog =
            ::serde_json::from_value(request["params"]["resolved_cte_catalog"].clone()).unwrap();
        let aliases = restored
            .try_map_cte_queries(|alias, _| {
                Ok::<_, std::convert::Infallible>(alias.as_str().to_owned())
            })
            .unwrap();
        assert_eq!(aliases, ["z_dependency", "a_visible"]);
        let mut order = Vec::new();
        restored.append_cte_parameter_order(&mut order);
        assert_eq!(order[0].as_str(), "shared");
    }
    assert_eq!(
        requests[0]["params"]["environments"]["SQL_FUN_POSTGRES_VERSION"],
        "17"
    );
    assert_eq!(
        requests[2]["params"]["environments"]["SQL_FUN_POSTGRES_VERSION"],
        "18"
    );
}

#[rstest::rstest]
#[case::initialization("initialize_schema_context", 1)]
#[case::analysis("analyze_query", 2)]
#[tokio::test]
async fn propagates_server_failures(#[case] method: &'static str, #[case] count: usize) {
    let project = test_helpers::Project::new();
    let (mut client, peer) = test_helpers::peer(Some(method)).await;
    let result = collect_submission(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first", "second"], QUERY, &test_helpers::catalog()),
    )
    .await;
    assert!(result.is_err());
    assert!(format!("{:#}", result.err().unwrap()).contains("injected failure"));
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), count);
}

#[tokio::test]
async fn sends_explicit_query_with_empty_catalog() {
    let project = test_helpers::Project::new();
    let catalog = ::sql_fun_core::ResolvedCteCatalog::default();
    let (mut client, peer) = test_helpers::peer(None).await;
    collect_submission(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first"], "SELECT 1", &catalog),
    )
    .await
    .unwrap();
    client.finish().await.unwrap();
    let requests = peer.await.unwrap();
    assert_eq!(
        requests[1]["params"]["resolved_cte_catalog"],
        ::serde_json::to_value(catalog).unwrap()
    );
    assert_eq!(requests[1]["params"]["query"]["Text"], "SELECT 1");
}

#[tokio::test]
#[ignore = "High CI disk usage from server builds"]
async fn recovered_catalog_reaches_real_server_realization() -> ::testresult::TestResult {
    // The real server/catalog setup is necessary to verify the receiving service.
    test_helpers::heavy::analyze_recovered_catalog().await
}

#[tokio::test]
async fn missing_context_handle_stops_before_query_submission() {
    let project = test_helpers::Project::new();
    let (mut client, peer) = test_helpers::peer(Some("missing_handle")).await;
    let result = collect_submission(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first"], QUERY, &test_helpers::catalog()),
    )
    .await;
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("no usable context handle")
    );
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_datasource_is_not_replaced_with_an_available_one() {
    let project = test_helpers::Project::new();
    let (mut client, peer) = test_helpers::peer(None).await;
    let result = collect_submission(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["missing"], QUERY, &test_helpers::catalog()),
    )
    .await;
    assert!(result.is_err());
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 0);
}

#[tokio::test]
async fn delivers_identity_before_completion_and_distinguishes_partial_failure() {
    let project = test_helpers::Project::new();
    let args = project.args();
    let catalog = test_helpers::catalog();
    let original_query = QUERY.to_owned();
    let fingerprint = catalog
        .finger_print(&::sql_fun_core::PlaceholderedSql::from_query(QUERY))
        .unwrap();
    let requirements = test_helpers::requirements(&["first", "first"], QUERY, &catalog);
    let (release, released) = ::tokio::sync::oneshot::channel();
    let (mut client, peer) = test_helpers::gated_second_failure(released).await;
    let (sender, mut receiver) = ::tokio::sync::mpsc::channel(1);
    let producer = submit(&mut client, &args, requirements, sender);
    let consumer = async {
        let first = receiver.recv().await.expect("first completed query");
        assert_eq!(first.function_name, "query_0");
        assert_eq!(first.original_query, original_query);
        assert_eq!(first.query_finger_print, fingerprint);
        assert_eq!(first.source_file, "src/queries.rs");
        assert_eq!(first.source_line, 42);
        assert_eq!(first.datasource, "first");
        assert!(first.response.queries().is_completed());
        release.send(()).unwrap();
        // A failed second RPC closes delivery, but is not successful completion.
        assert!(receiver.recv().await.is_none());
    };
    let (completion, ()) = ::tokio::time::timeout(std::time::Duration::from_secs(5), async {
        ::tokio::join!(producer, consumer)
    })
    .await
    .expect("incremental channel delivery must not wait for the full batch");
    assert!(format!("{:#}", completion.unwrap_err()).contains("injected failure"));
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 3);
}

#[tokio::test]
async fn closed_consumer_is_a_delivery_failure() {
    let project = test_helpers::Project::new();
    let (mut client, peer) = test_helpers::peer(None).await;
    let (sender, receiver) = ::tokio::sync::mpsc::channel(1);
    drop(receiver);
    let result = submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first", "first"], QUERY, &test_helpers::catalog()),
        sender,
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("receiver closed before delivery")
    );
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 2);
}

#[rstest::rstest]
#[case::ordinary(false)]
#[case::force(true)]
#[tokio::test]
async fn check_session_regenerates_multiple_statements_and_removes_stale_entries(
    #[case] force: bool,
) {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    use ::sql_fun_core::lockfile::{Lockfile, LockfileEntry, LockfileStatement};
    let project = test_helpers::Project::new();
    let args = project.args();
    let path = args.lockfile_path("first").unwrap();
    project.write_lockfile(
        "first",
        "old hash",
        &[LockfileEntry::new(
            "stale",
            "removed",
            &[LockfileStatement::new("SELECT old", &[], &[])],
        )
        .unwrap()],
    );
    let requirements = test_helpers::requirements(
        &["first", "first"],
        "SELECT 0; SELECT 1",
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, force, false).unwrap();
    let (client, peer) =
        test_helpers::peer_with_response(test_helpers::successful_response(2), None).await;
    let mut output = Vec::new();
    let mut progress = Progress::new(&mut output, ProgressMode::dots());
    let result = CheckArgs::try_parse_from(["check"])
        .unwrap()
        .process_analyses(client, &args, requirements, lockfiles, &mut progress)
        .await
        .unwrap();
    progress
        .finish(result == std::process::ExitCode::SUCCESS)
        .unwrap();
    assert_eq!(result, std::process::ExitCode::SUCCESS);
    assert_eq!(peer.await.unwrap().len(), 3);
    let hash = project.schema_hash();
    let reader = Lockfile::from_file(&path, &hash).unwrap();
    assert!(reader.get_entry("stale").unwrap().is_none());
    let content = std::fs::read_to_string(path).unwrap();
    assert_eq!(content.matches("[[entries]]").count(), 2);
    assert_eq!(content.matches("[[entries.statements]]").count(), 4);
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Analyzing 2 query functions\n..\nWriting lockfiles and inventories\nCompleted: 2/2 query functions analyzed\n"
    );
}

#[rstest::rstest]
#[case::initialization("initialize_schema_context")]
#[case::first_analysis("analyze_query")]
#[case::after_partial_results("second_analysis")]
#[case::connection_completion("finish")]
#[tokio::test]
async fn check_session_does_not_write_on_rpc_failure(#[case] failure: &'static str) {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    let project = test_helpers::Project::new();
    let args = project.args();
    let path = args.lockfile_path("first").unwrap();
    project.write_lockfile("first", "old hash", &[]);
    let old = std::fs::read_to_string(&path).unwrap();
    let inventory_path = args
        .project_dir()
        .unwrap()
        .join(format!("sql-fun.first.{}.toml", args.schema()));
    std::fs::write(&inventory_path, "prior inventory").unwrap();
    let requirements = test_helpers::requirements(
        &["first", "first"],
        "SELECT 1",
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false).unwrap();
    let (client, peer) =
        test_helpers::peer_with_response(test_helpers::successful_response(1), Some(failure)).await;
    let mut output = Vec::new();
    let mut progress = Progress::new(&mut output, ProgressMode::dots());
    let result = CheckArgs::try_parse_from(["check", "--force-rebuild"])
        .unwrap()
        .process_analyses(client, &args, requirements, lockfiles, &mut progress)
        .await;
    progress.finish(false).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), old);
    assert_eq!(
        std::fs::read_to_string(inventory_path).unwrap(),
        "prior inventory"
    );
    peer.await.unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Stopped:"));
    assert!(!output.contains("Writing") && !output.contains("Completed:"));
    if failure == "second_analysis" {
        assert!(output.contains(".\nStopped: 1/2"));
    }
}

#[rstest::rstest]
#[case::single(1, 0)]
#[case::later_statement(2, 1)]
#[tokio::test]
async fn completed_ineligible_analysis_removes_old_entries_and_returns_failure(
    #[case] count: usize,
    #[case] failed: usize,
) {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    let project = test_helpers::Project::new();
    let args = project.args();
    project.write_lockfile(
        "first",
        &project.schema_hash(),
        &[::sql_fun_core::lockfile::LockfileEntry::new(
            "stale",
            "removed",
            &[::sql_fun_core::lockfile::LockfileStatement::new(
                "SELECT old",
                &[],
                &[],
            )],
        )
        .unwrap()],
    );
    let requirements = test_helpers::requirements(
        &["first"],
        "invalid SQL",
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false).unwrap();
    let mut response = test_helpers::successful_response(count);
    response["queries"]["items"][failed]["complete"] = ::serde_json::json!(false);
    response["queries"]["items"][failed]["execute_sql"] = ::serde_json::Value::Null;
    response["queries"]["items"][failed]["column_access"] = ::serde_json::json!({
        "handle":"accesses", "total_length":1, "offset":0, "require_release":false,
        "items":[{"access_kind":"ReadRows", "relation_kind":"View", "relation":{"catalog":null,"schema":"public","name":"visible_view"}, "column_name":null}]
    });
    let (client, peer) = test_helpers::peer_with_response(response, None).await;
    let mut progress = Progress::new(Vec::new(), ProgressMode::silent());
    let result = CheckArgs::try_parse_from(["check"])
        .unwrap()
        .process_analyses(client, &args, requirements, lockfiles, &mut progress)
        .await
        .unwrap();
    assert_eq!(result, std::process::ExitCode::FAILURE);
    let content = std::fs::read_to_string(args.lockfile_path("first").unwrap()).unwrap();
    assert!(content.contains("entries = []"));
    let inventory = std::fs::read_to_string(
        args.project_dir()
            .unwrap()
            .join(format!("sql-fun.first.{}.toml", args.schema())),
    )
    .unwrap();
    assert!(inventory.contains("visible_view") && inventory.contains("ReadRows"));
    peer.await.unwrap();
}

#[rstest::rstest]
#[case::absent_empty(false, 0, false, true)]
#[case::valid_empty(true, 0, false, true)]
#[case::bad_hash_empty(true, 0, true, false)]
#[case::absent_nonempty(false, 1, false, false)]
#[case::unchanged(true, 1, false, true)]
#[tokio::test]
async fn validate_only_compares_prospective_state_without_mutation(
    #[case] exists: bool,
    #[case] statements: usize,
    #[case] old_hash: bool,
    #[case] expected_match: bool,
) {
    use crate::check::{lockfiles::Lockfiles, submission::QueryAnalysis};
    use ::sql_fun_client::payloads::AnalyzeQueryResponse;
    use ::sql_fun_core::lockfile::{LockfileDocument, LockfileEntry, LockfileStatement};
    let project = test_helpers::Project::new();
    let args = project.args();
    let path = args.lockfile_path("first").unwrap();
    let hash = project.schema_hash();
    if exists {
        let mut old = LockfileDocument::new(if old_hash { "wrong hash" } else { &hash });
        if statements != 0 {
            old.push(
                LockfileEntry::new(
                    "fingerprint",
                    "query",
                    &[LockfileStatement::new("SELECT 0", &[], &[])],
                )
                .unwrap(),
            );
        }
        LockfileDocument::replace(&path, &old.encode().unwrap()).unwrap();
    }
    let before = std::fs::read(&path).ok();
    let requirements = test_helpers::requirements(
        &["first"],
        "SELECT 0",
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let mut lockfiles = Lockfiles::new(&args, &requirements, false, false).unwrap();
    if statements != 0 {
        let response: AnalyzeQueryResponse =
            ::serde_json::from_value(test_helpers::successful_response(statements)).unwrap();
        let analysis = QueryAnalysis {
            function_name: "query".to_owned(),
            original_query: "SELECT 0".to_owned(),
            query_finger_print: "fingerprint".to_owned(),
            source_file: "queries.rs".to_owned(),
            source_line: 1,
            datasource: "first".to_owned(),
            response,
            tokenizations: PreparedDiagnostics::default(),
        };
        lockfiles.receive(&analysis).unwrap();
    }
    assert_eq!(lockfiles.finalize(true).unwrap(), expected_match);
    assert_eq!(std::fs::read(path).ok(), before);
}

#[tokio::test]
async fn conversion_failure_drains_delivery_without_writing_partial_inventory() {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    let project = test_helpers::Project::new();
    let args = project.args();
    project.write_lockfile("first", &project.schema_hash(), &[]);
    let path = args.lockfile_path("first").unwrap();
    let before = std::fs::read(&path).unwrap();
    // More results than channel capacity proves a failed consumer still drains delivery.
    let requirements = test_helpers::requirements(
        &["first"; 12],
        "SELECT 0",
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false).unwrap();
    let mut response = test_helpers::successful_response(1);
    response["queries"]["items"][0]["parameters"] = ::serde_json::json!({
        "handle":"parameters", "total_length":1, "offset":0, "require_release":false,
        "items":[{"name":"p", "spans":[], "parameter_index":65536, "parameter_type":{"schema_name":"pg_catalog", "local_name":"int4", "is_array":false}, "nullability":"Unknown"}]
    });
    let (client, peer) = test_helpers::peer_with_response(response, None).await;
    let mut output = Vec::new();
    let mut progress = Progress::new(&mut output, ProgressMode::dots());
    let options = CheckArgs::try_parse_from(["check"]).unwrap();
    let result = ::tokio::time::timeout(
        std::time::Duration::from_secs(5),
        options.process_analyses(client, &args, requirements, lockfiles, &mut progress),
    )
    .await
    .expect("failed mapping must not deadlock result delivery");
    progress.finish(false).unwrap();
    assert!(format!("{:#}", result.unwrap_err()).contains("parameter index exceeds"));
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(peer.await.unwrap().len(), 13);
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("............\nStopped: 12/12")
    );
}

#[tokio::test]
async fn check_renders_all_diagnostics_for_incomplete_multistatement_query()
-> ::testresult::TestResult {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    let project = test_helpers::Project::new();
    let args = project.args();
    let query = "SELECT 1; SELECT α FROM missing /* original comment */";
    let statement = "SELECT α FROM missing /* original comment */";
    let requirements = test_helpers::requirements(
        &["first"],
        query,
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false)?;
    let response = test_helpers::incomplete_diagnostic_response(statement);
    let (client, peer) = test_helpers::peer_with_response(response, None).await;
    let mut progress = Progress::new(Vec::new(), ProgressMode::silent());
    let mut diagnostics = Vec::new();
    let status = CheckArgs::try_parse_from(["check"])?
        .process_analyses_to(
            client,
            &args,
            requirements,
            lockfiles,
            &mut progress,
            &mut diagnostics,
        )
        .await?;
    assert_eq!(status, std::process::ExitCode::FAILURE);
    let output = String::from_utf8(diagnostics)?;
    assert_eq!(output.matches("Original SQL:\n").count(), 4);
    assert_eq!(output.matches(query).count(), 4);
    assert_eq!(
        output
            .matches("SQL query query_0 at src/queries.rs:42")
            .count(),
        4
    );
    for expected in [
        "domain_base_type_not_found",
        "unreachable_coalesce_argument",
        "parameter_constraint",
        "extension_not_found",
        "extension not found: missing_extension",
        "base type name",
        "consider a type hint",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
    assert!(output.contains("⚠"), "warning severity: {output}");
    assert!(output.contains("☞"), "advice severity: {output}");
    assert!(
        output.contains("SQL:1:16"),
        "UTF-8 source position: {output}"
    );
    assert!(output.contains(statement));
    let locationless = output
        .rsplit("SQL query query_0 at src/queries.rs:42")
        .next()
        .unwrap();
    assert!(!locationless.contains("╭─[SQL:"));
    let requests = peer.await?;
    assert_eq!(
        requests
            .iter()
            .filter(|request| request["method"] == "tokenize_sql")
            .count(),
        1
    );
    assert_eq!(requests[2]["params"]["source"], statement);
    assert_eq!(requests[2]["params"]["dialect"], "postgres");
    let lockfile = std::fs::read_to_string(args.lockfile_path("first")?)?;
    assert!(lockfile.contains("entries = []"));
    Ok(())
}

#[tokio::test]
async fn diagnostics_keep_each_query_function_identity() -> ::testresult::TestResult {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    use ::sql_fun_client::payloads::{AnalysisProbrem, DiagnosticSeverity};
    use ::sql_fun_core::analysis_problem::AnalysisProblem;

    let project = test_helpers::Project::new();
    let args = project.args();
    let source = "SELECT 1";
    let requirements = test_helpers::requirements(
        &["first", "first"],
        source,
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false)?;
    let problem = AnalysisProblem::unreachable_coalesce_argument(
        ::sql_fun_core::analysis_problem::SemScalarExpr::new("literal", None, None),
        ::sql_fun_core::analysis_problem::SemScalarExpr::new("literal", None, Some(true)),
    );
    let mut response = test_helpers::successful_response(1);
    response["queries"]["items"][0]["probrems"] =
        test_helpers::diagnostic_page(&[AnalysisProbrem::new(
            problem.code(),
            DiagnosticSeverity::warning(),
            std::path::Path::new("<query>"),
            None,
        )
        .with_semantic_problem(&problem)]);
    let (client, peer) = test_helpers::peer_with_response(response, None).await;
    let mut progress = Progress::new(Vec::new(), ProgressMode::silent());
    let mut diagnostics = Vec::new();
    let status = CheckArgs::try_parse_from(["check"])?
        .process_analyses_to(
            client,
            &args,
            requirements,
            lockfiles,
            &mut progress,
            &mut diagnostics,
        )
        .await?;
    assert_eq!(status, std::process::ExitCode::SUCCESS);
    let output = String::from_utf8(diagnostics)?;
    assert_eq!(
        output
            .matches("SQL query query_0 at src/queries.rs:42")
            .count(),
        1
    );
    assert_eq!(
        output
            .matches("SQL query query_1 at src/queries.rs:42")
            .count(),
        1
    );
    assert_eq!(output.matches("Original SQL:\nSELECT 1").count(), 2);
    assert_eq!(output.matches("unreachable_coalesce_argument").count(), 2);
    assert_eq!(peer.await?.len(), 3);
    Ok(())
}

#[tokio::test]
async fn tokenization_failure_still_renders_received_diagnostic() -> ::testresult::TestResult {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    use ::sql_fun_client::payloads::{AnalysisProbrem, DiagnosticSeverity};
    use ::sql_fun_core::{DiagnosticPresentation, SqlDialect, analysis_problem::AnalysisProblem};

    let project = test_helpers::Project::new();
    let args = project.args();
    let source = "SELECT α /* comment */";
    let requirements = test_helpers::requirements(
        &["first"],
        source,
        &::sql_fun_core::ResolvedCteCatalog::default(),
    );
    let lockfiles = Lockfiles::new(&args, &requirements, false, false)?;
    let problem = AnalysisProblem::extension_not_found("missing_extension");
    let presentation = DiagnosticPresentation::new(&problem, source, SqlDialect::PostgreSQL, 0);
    let mut response = test_helpers::successful_response(1);
    response["queries"]["items"][0]["probrems"] =
        test_helpers::diagnostic_page(&[AnalysisProbrem::new(
            problem.code(),
            DiagnosticSeverity::error(),
            std::path::Path::new("<query>"),
            None,
        )
        .with_presentation(Some(&presentation))]);
    let (client, peer) = test_helpers::peer_with_response(response, Some("tokenize_sql")).await;
    let mut progress = Progress::new(Vec::new(), ProgressMode::silent());
    let mut diagnostics = Vec::new();
    let failure = CheckArgs::try_parse_from(["check"])?
        .process_analyses_to(
            client,
            &args,
            requirements,
            lockfiles,
            &mut progress,
            &mut diagnostics,
        )
        .await
        .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("cannot tokenize SQL diagnostic")
    );
    let output = String::from_utf8(diagnostics)?;
    assert!(output.contains("extension not found: missing_extension"));
    assert!(output.contains(source));
    assert_eq!(peer.await?.len(), 3);
    Ok(())
}

#[tokio::test]
async fn check_preserves_expanded_server_sql_in_lockfile() {
    use crate::check::{
        CheckArgs,
        lockfiles::Lockfiles,
        progress::{Progress, ProgressMode},
    };
    use ::clap::Parser;
    use ::sql_fun_core::lockfile::Lockfile;

    let project = test_helpers::Project::new();
    project.enable_asterisk_expansion();
    let args = project.args();
    let catalog = ::sql_fun_core::ResolvedCteCatalog::default();
    let original = "SELECT *, ${lookup:int4} AS requested FROM entries WHERE id = ${lookup:int4}";
    let executable = "SELECT entries.id AS id, $1 AS requested FROM entries WHERE id = $1";
    let fingerprint = catalog
        .finger_print(&::sql_fun_core::PlaceholderedSql::from_query(original))
        .unwrap();
    let requirements = test_helpers::requirements(&["first"], original, &catalog);
    let lockfiles = Lockfiles::new(&args, &requirements, false, false).unwrap();
    let (client, peer) = test_helpers::peer_with_response(
        test_helpers::expanded_projection_response(executable),
        None,
    )
    .await;
    let mut output = Vec::new();
    let mut progress = Progress::new(&mut output, ProgressMode::dots());
    let result = CheckArgs::try_parse_from(["check"])
        .unwrap()
        .process_analyses(client, &args, requirements, lockfiles, &mut progress)
        .await
        .unwrap();
    progress
        .finish(result == std::process::ExitCode::SUCCESS)
        .unwrap();
    assert_eq!(result, std::process::ExitCode::SUCCESS);

    let requests = peer.await.unwrap();
    assert_eq!(requests[1]["params"]["query"]["Text"], original);
    let transferred = ::sql_fun_core::SqlFunArgs::from_environments(
        &::serde_json::from_value(requests[0]["params"]["environments"].clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        transferred
            .selected_metadata("first")
            .unwrap()
            .rewrite_options()
            .expand_asterisk(),
        Some(true)
    );

    let path = args.lockfile_path("first").unwrap();
    let reader = Lockfile::from_file(&path, &project.schema_hash()).unwrap();
    let entry = reader.get_entry(&fingerprint).unwrap().unwrap();
    let statement = entry.single_statement().unwrap();
    assert_eq!(statement.execute_sql(), executable);
    assert_eq!(statement.bind_parameters().len(), 1);
    assert_eq!(statement.bind_parameters()[0].name(), "lookup");
    assert_eq!(statement.bind_parameters()[0].parameter_index(), 1);
    assert_eq!(statement.result_set_shape()[0].field_name(), "id");
    assert_eq!(statement.result_set_shape()[0].field_index(), 0);
    assert_eq!(statement.result_set_shape()[1].field_name(), "requested");
    assert_eq!(statement.result_set_shape()[1].field_index(), 1);
}
