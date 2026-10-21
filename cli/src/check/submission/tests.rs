use super::{submit, test_helpers};

#[tokio::test]
async fn submits_recovered_catalogs_with_datasource_scoped_contexts() {
    let project = test_helpers::Project::new();
    let catalog = test_helpers::catalog();
    let expected = ::serde_json::to_value(&catalog).unwrap();
    let (mut client, peer) = test_helpers::peer(None).await;
    let analyses = submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first", "second", "first"], &catalog),
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
    for (index, datasource) in [(1, "first"), (3, "second"), (4, "first")] {
        let request = &requests[index];
        assert_eq!(request["method"], "analyze_query");
        assert_eq!(
            request["params"]["context_handle"],
            format!("context-{datasource}")
        );
        assert!(request["params"]["query"].is_null());
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
    let result = submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first", "second"], &test_helpers::catalog()),
    )
    .await;
    assert!(result.is_err());
    assert!(format!("{:#}", result.err().unwrap()).contains("injected failure"));
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), count);
}

#[tokio::test]
async fn sends_base_only_catalog_without_a_second_query_input() {
    let project = test_helpers::Project::new();
    let catalog = ::sql_fun_core::ResolvedCteCatalog::from(
        &::sql_fun_core::PlaceholderedSql::from_query("SELECT 1"),
    );
    let (mut client, peer) = test_helpers::peer(None).await;
    submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first"], &catalog),
    )
    .await
    .unwrap();
    client.finish().await.unwrap();
    let requests = peer.await.unwrap();
    assert_eq!(
        requests[1]["params"]["resolved_cte_catalog"],
        ::serde_json::to_value(catalog).unwrap()
    );
    assert!(requests[1]["params"]["query"].is_null());
}

#[tokio::test]
async fn recovered_catalog_reaches_real_server_realization() -> ::testresult::TestResult {
    // The real server/catalog setup is necessary to verify the receiving service.
    test_helpers::heavy::analyze_recovered_catalog().await
}

#[tokio::test]
async fn missing_context_handle_stops_before_query_submission() {
    let project = test_helpers::Project::new();
    let (mut client, peer) = test_helpers::peer(Some("missing_handle")).await;
    let result = submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["first"], &test_helpers::catalog()),
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
    let result = submit(
        &mut client,
        &project.args(),
        test_helpers::requirements(&["missing"], &test_helpers::catalog()),
    )
    .await;
    assert!(result.is_err());
    client.finish().await.unwrap();
    assert_eq!(peer.await.unwrap().len(), 0);
}
