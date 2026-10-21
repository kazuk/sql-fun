mod test_helpers;

use self::test_helpers::query_project;
use crate::test_helpers::heavy;
use ::serde_json::{Value, json};

#[rstest::rstest]
#[case::select("SELECT id, value FROM entries WHERE id = $1", "Select", 2)]
#[case::insert("INSERT INTO entries(id) VALUES ($1) RETURNING id", "Insert", 1)]
#[case::update("UPDATE entries SET value = $1 RETURNING id, value", "Update", 2)]
#[case::delete("DELETE FROM entries WHERE id = $1 RETURNING id", "Delete", 1)]
#[case::insert_no_return("INSERT INTO entries(id) VALUES ($1)", "Insert", 0)]
#[case::update_no_return("UPDATE entries SET value = $1", "Update", 0)]
#[case::delete_no_return("DELETE FROM entries WHERE id = $1", "Delete", 0)]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_analyzes_typed_select_and_dml(
    #[case] sql: &str,
    #[case] kind: &str,
    #[case] columns: usize,
) {
    let project = query_project();
    // A real authenticated socket is necessary to verify session dispatch and paging.
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4 NOT NULL, value int4);"),
        )
        .await;
    let response = connection.rpc("analyze_query", &json!({"context_handle":initialized["result"]["context_handle"], "query":{"Text":sql}})).await;
    let item = &response["result"]["queries"]["items"][0];
    assert_eq!(item["query_kind"], kind, "{response}");
    assert_eq!(item["complete"], true, "{response}");
    assert_eq!(item["parameters"]["items"][0]["parameter_index"], 1);
    assert_eq!(
        item["parameters"]["items"][0]["parameter_type"]["local_name"],
        "int4"
    );
    assert_eq!(
        item["result_columns"]["items"].as_array().unwrap().len(),
        columns
    );
    assert!(item["execute_sql"].as_str().unwrap().contains("$1"));
    if columns > 0 {
        assert_eq!(item["result_columns"]["items"][0]["ordinal"], 0);
        assert_eq!(item["result_columns"]["items"][0]["nullability"], "NotNull");
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_preserves_numbering_and_session_lifecycle() {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4, value int4);"),
        )
        .await;
    let handle = &initialized["result"]["context_handle"];
    let response = connection.rpc("analyze_query", &json!({"context_handle":handle, "query":{"Text":"SELECT $3::int4 AS third, $1::int4 AS first, $3::int4 AS again; SELECT 2 AS second"}})).await;
    let queries = &response["result"]["queries"];
    assert_eq!(queries["total_length"], 2, "{response}");
    let items = queries["items"].as_array().unwrap();
    let parameters = items[0]["parameters"]["items"].as_array().unwrap();
    assert_eq!(
        parameters
            .iter()
            .map(|parameter| parameter["parameter_index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(items[1]["parameters"]["total_length"], 0);
    test_helpers::verify_query_collection_lifecycle(&mut connection, queries).await;
    let mut foreign = heavy::Connection::start(16384, 2).await;
    foreign.authenticate().await;
    assert_eq!(
        foreign
            .rpc(
                "analyze_query",
                &json!({"context_handle":handle, "query":{"Text":"SELECT 1"}})
            )
            .await["error"]["data"],
        "InvalidContextHandle"
    );
    for sql in [
        "SELECT (",
        "SELECT missing FROM entries",
        "SELECT $1 IS NULL",
    ] {
        let response = connection
            .rpc(
                "analyze_query",
                &json!({"context_handle":handle, "query":{"Text":sql}}),
            )
            .await;
        assert_eq!(
            response["result"]["queries"]["items"][0]["complete"], false,
            "{response}"
        );
        assert!(
            response["result"]["queries"]["items"][0]["probrems"]["total_length"]
                .as_u64()
                .unwrap()
                > 0,
            "{response}"
        );
    }
    let response = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":handle, "query":{"Text":"SELECT 1"}}),
        )
        .await;
    assert_eq!(
        response["result"]["queries"]["items"][0]["complete"], true,
        "{response}"
    );
    foreign.end_requests().await;
    foreign.finish().await.unwrap();
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

fn catalog(base: &str, ctes: &[(&str, &str)]) -> Value {
    let mut parameters = ::sql_fun_core::CteParameterCollection::default();
    let mut queries = ::indexmap::IndexMap::new();
    for (alias, sql) in ctes {
        let query = ::sql_fun_core::PlaceholderedSql::from_query(sql);
        for (name, hints) in query.parameters() {
            parameters.push_query_parameter(&::sql_fun_core::CteName::new(alias), name, hints);
        }
        queries.insert(::sql_fun_core::CteAlias::new(alias), query);
    }
    ::serde_json::to_value(::sql_fun_core::ResolvedCteCatalog::new(
        &::sql_fun_core::PlaceholderedSql::from_query(base),
        &parameters,
        &queries,
    ))
    .unwrap()
}

#[rstest::rstest]
#[case::expansion("SELECT ${base:int4}, id FROM chosen", &[ ("chosen", "SELECT ${cte:int4} AS id") ], 1, true)]
#[case::base_only("SELECT ${base:int4}; SELECT 2", &[], 2, true)]
#[case::invalid_expansion("SELECT missing FROM chosen", &[ ("chosen", "SELECT 1 AS id") ], 1, false)]
#[case::multiple_expanded("SELECT 1; SELECT 2", &[ ("chosen", "SELECT 1 AS id") ], 1, false)]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_catalog_input(
    #[case] base: &str,
    #[case] ctes: &[(&str, &str)],
    #[case] length: usize,
    #[case] complete: bool,
) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let response = connection.rpc("analyze_query", &json!({"context_handle":initialized["result"]["context_handle"], "resolved_cte_catalog":catalog(base, ctes)})).await;
    let queries = &response["result"]["queries"];
    assert_eq!(queries["total_length"], length, "{response}");
    assert_eq!(queries["items"][0]["complete"], complete, "{response}");
    if complete && !ctes.is_empty() {
        let item = &queries["items"][0];
        assert!(item["execute_sql"].as_str().unwrap().contains("WITH"));
        assert_eq!(item["parameters"]["items"][0]["name"], "cte");
        assert_eq!(item["parameters"]["items"][0]["parameter_index"], 1);
        assert_eq!(item["parameters"]["items"][1]["name"], "base");
        assert_eq!(item["parameters"]["items"][1]["parameter_index"], 2);
    }
    if !complete && base.contains("missing") {
        let problem = &queries["items"][0]["probrems"]["items"][0];
        assert_eq!(problem["code"], "column_not_found_in_source", "{response}");
        assert!(problem["presentation"].is_null());
        assert!(problem["statement_relative_span"].is_null());
        assert!(!problem["semantic_problem"].is_null());
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::unknown_column("SELECT missing FROM entries", "column_not_found_in_source")]
#[case::unknown_relation("SELECT id FROM missing", "relation_not_found")]
#[case::unresolved_parameter("SELECT $1 IS NULL", "parameter_type_unresolved")]
#[case::invalid_hint("SELECT ${id:no_such_type}", "invalid_parameter_hint")]
#[case::contradiction("SELECT 1 HAVING $1 LIMIT $1", "parameter_constraint")]
#[case::mixed_parameters("SELECT ${id:int4}, $1", "query_realization")]
#[case::zero_parameter("SELECT $0::int4", "parameter_constraint")]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_reports_distinct_analysis_failures(#[case] sql: &str, #[case] code: &str) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let response = connection.rpc("analyze_query", &json!({"context_handle":initialized["result"]["context_handle"], "query":{"Text":sql}})).await;
    let item = &response["result"]["queries"]["items"][0];
    assert_eq!(item["complete"], false, "{response}");
    assert!(
        item["probrems"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|problem| problem["code"] == code),
        "{response}"
    );
    if code == "column_not_found_in_source" {
        assert!(item["result_columns"]["items"][0]["column_type"].is_null());
        assert_eq!(
            item["probrems"]["total_length"], 1,
            "missing result type must not add a server diagnostic"
        );
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_preserves_source_spans_and_independent_hint_state() {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let handle = &initialized["result"]["context_handle"];
    let sql = "SELECT ${first:int4}, ${first}; SELECT ${second:text}, ';' -- ; ignored\n";
    let response = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":handle, "query":{"Text":sql}}),
        )
        .await;
    let items = &response["result"]["queries"]["items"];
    assert_eq!(items.as_array().unwrap().len(), 2, "{response}");
    assert_eq!(
        items[0]["parameters"]["items"][0]["spans"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        items[0]["parameters"]["items"][0]["spans"][0]["from"]["offset"],
        7
    );
    assert_eq!(
        items[0]["parameters"]["items"][0]["spans"][0]["to"]["offset"],
        20
    );
    assert_eq!(items[1]["parameters"]["items"][0]["parameter_index"], 2);
    assert_eq!(
        items[1]["parameters"]["items"][0]["parameter_type"]["local_name"],
        "text"
    );
    let unhinted = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":handle, "query":{"Text":"SELECT $1 IS NULL"}}),
        )
        .await;
    assert!(
        unhinted["result"]["queries"]["items"][0]["parameters"]["items"][0]["parameter_type"]
            .is_null()
    );
    let empty = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":handle, "query":{"Text":"-- empty"}}),
        )
        .await;
    assert_eq!(empty["result"]["queries"]["total_length"], 0);
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_accepts_only_explicit_input_and_retains_file_results() {
    let project = query_project();
    let path = test_helpers::query_file(&project, "SELECT ${id:int4}; SELECT 2");
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let handle = &initialized["result"]["context_handle"];
    let response = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":handle, "query":{"File":path}}),
        )
        .await;
    let queries = &response["result"]["queries"];
    assert_eq!(queries["total_length"], 2, "{response}");
    std::fs::remove_file(&path).unwrap();
    let page = connection
        .rpc(
            "read_collection",
            &json!({"collection_handle":queries["handle"], "start_offset":0, "count":null}),
        )
        .await;
    assert_eq!(page["result"], *queries);
    for params in [
        json!({"context_handle":handle}),
        json!({"context_handle":handle,"query":{"Url":"https://example.invalid/query.sql"}}),
        json!({"context_handle":handle,"query":{"Text":"SELECT 1"},"resolved_cte_catalog":catalog("SELECT 2", &[])}),
    ] {
        let response = connection.rpc("analyze_query", &params).await;
        assert!(
            response["error"]["data"]
                .get("InvalidAnalysisInput")
                .is_some(),
            "{response}"
        );
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::inferred("SELECT $1 + 1 AS value")]
#[case::case("SELECT CASE WHEN ${condition:bool} THEN ${value:int4} ELSE 1 END AS value")]
#[case::coalesce("SELECT coalesce(${value:int4}, 1) AS value")]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_reuses_existing_expression_type_analysis(#[case] sql: &str) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let response = connection.rpc("analyze_query", &json!({"context_handle":initialized["result"]["context_handle"], "query":{"Text":sql}})).await;
    let item = &response["result"]["queries"]["items"][0];
    assert_eq!(item["complete"], true, "{response}");
    assert_eq!(
        item["result_columns"]["items"][0]["column_type"]["local_name"],
        "int4"
    );
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::ddl("CREATE TABLE scratch(id int4)", "Other", true, 0)]
#[case::merge(
    "MERGE INTO entries t USING entries s ON true WHEN MATCHED THEN DO NOTHING",
    "Merge",
    true,
    0
)]
#[case::merge_returning(
    "MERGE INTO entries t USING entries s ON true WHEN MATCHED THEN DELETE RETURNING t.id",
    "Merge",
    true,
    1
)]
#[case::truncate("TRUNCATE entries", "Truncate", false, 0)]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_classifies_valid_statement_kinds(
    #[case] sql: &str,
    #[case] kind: &str,
    #[case] complete: bool,
    #[case] columns: usize,
) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let response = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":initialized["result"]["context_handle"],"query":{"Text":sql}}),
        )
        .await;
    let item = &response["result"]["queries"]["items"][0];
    assert_eq!(item["query_kind"], kind, "{response}");
    assert_eq!(item["complete"], complete, "{response}");
    assert_eq!(
        item["result_columns"]["total_length"], columns,
        "{response}"
    );
    if complete {
        assert!(item["execute_sql"].is_string(), "{response}");
        assert_eq!(item["probrems"]["total_length"], 0, "{response}");
    } else {
        // Classification survives the existing common-AST conversion limitation.
        assert_eq!(
            item["probrems"]["items"][0]["code"], "unsupported_statement",
            "{response}"
        );
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[rstest::rstest]
#[case::native("SELECT 1; SELECT (")]
#[case::named("SELECT ${id:int4}; SELECT (")]
#[tokio::test(flavor = "multi_thread")]
async fn query_rpc_preserves_failed_statement_source(#[case] sql: &str) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &project.rpc_request("CREATE TABLE entries(id int4);"),
        )
        .await;
    let response = connection
        .rpc(
            "analyze_query",
            &json!({"context_handle":initialized["result"]["context_handle"],"query":{"Text":sql}}),
        )
        .await;
    let items = response["result"]["queries"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{response}");
    let item = &items[1];
    let from = usize::try_from(item["query_span"]["from"]["offset"].as_u64().unwrap()).unwrap();
    let to = usize::try_from(item["query_span"]["to"]["offset"].as_u64().unwrap()).unwrap();
    assert!(from > sql.find(';').unwrap(), "{response}");
    assert_eq!(sql[from..to].trim(), "SELECT (");
    let presentation = &item["probrems"]["items"][0]["presentation"];
    assert_eq!(presentation["source"], &sql[from..to]);
    assert_eq!(presentation["statement_offset"], from);
    connection.end_requests().await;
    connection.finish().await.unwrap();
}
