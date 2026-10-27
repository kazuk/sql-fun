use super::{
    catalog,
    test_helpers::{QueryProblems, query_project, rewrite_request},
};
use crate::test_helpers::heavy;

const SCHEMA: &str =
    "CREATE TABLE entries(id int4 NOT NULL, value int4); CREATE TABLE other(id int4, extra int4);";

#[rstest::rstest]
#[case::table("SELECT * FROM entries", 2, false)]
#[case::qualified("SELECT e.* FROM entries e", 2, false)]
#[case::repeated("SELECT *, e.* FROM entries e", 4, false)]
#[case::nested("SELECT * FROM (SELECT * FROM entries) s", 2, true)]
#[case::using_join("SELECT * FROM entries JOIN other USING (id)", 3, false)]
#[case::full_join("SELECT * FROM entries FULL JOIN other USING (id)", 3, false)]
#[case::natural_join("SELECT * FROM entries NATURAL JOIN other", 3, false)]
#[case::duplicate_join_alias("SELECT j.*, j.value FROM (entries JOIN other ON true) j", 5, true)]
#[case::correlated_duplicate_join_alias(
    "SELECT EXISTS (SELECT j.*) FROM (entries CROSS JOIN other) AS j",
    1,
    true
)]
#[case::insert("INSERT INTO entries(id) VALUES (1) RETURNING *", 2, false)]
#[case::update("UPDATE entries SET value = 1 RETURNING entries.*", 2, false)]
#[case::delete("DELETE FROM entries RETURNING *", 2, false)]
#[tokio::test(flavor = "multi_thread")]
async fn expands_server_sql_at_the_responsible_query_level(
    #[case] sql: &str,
    #[case] width: usize,
    #[case] retains_fixed_star: bool,
) {
    let project = query_project();
    // The socket exercises policy capture, semantic expansion, and output rendering together.
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let initialized = connection
        .rpc(
            "initialize_schema_context",
            &rewrite_request(&project, SCHEMA, Some(true)),
        )
        .await;
    let response = connection.rpc("analyze_query", &::serde_json::json!({"context_handle": initialized["result"]["context_handle"], "query": {"Text": sql}})).await;
    let item = &response["result"]["queries"]["items"][0];
    assert_eq!(item["complete"], true, "{response}");
    assert_eq!(
        item["result_columns"]["items"].as_array().unwrap().len(),
        width
    );
    let output = item["execute_sql"].as_str().unwrap();
    assert_eq!(output.contains('*'), retains_fixed_star, "{output}");
    assert!(output.to_ascii_lowercase().contains(" as "), "{output}");
    let parsed =
        ::sql_fun_sqlast::parse_statements(output, ::sql_fun_sqlast::ast::Dialect::postgres())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    assert_eq!(parsed.len(), 1);
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn captures_policy_per_handle_without_polluting_the_shared_schema_cache() {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let mut handles = Vec::new();
    for enabled in [Some(true), Some(false), None, Some(true)] {
        let initialized = connection
            .rpc(
                "initialize_schema_context",
                &rewrite_request(&project, SCHEMA, enabled),
            )
            .await;
        handles.push((initialized["result"]["context_handle"].clone(), enabled));
    }
    // Read old handles after all metadata edits: policy is captured, never reloaded or cached globally.
    for (handle, enabled) in handles {
        let response = connection
            .rpc(
                "analyze_query",
                &::serde_json::json!({"context_handle": handle, "query": {"Text": "SELECT * FROM entries"}}),
            )
            .await;
        let item = &response["result"]["queries"]["items"][0];
        assert_eq!(item["complete"], true, "{response}");
        assert_eq!(
            item["execute_sql"].as_str().unwrap().contains('*'),
            enabled != Some(true)
        );
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reparses_imported_ctes_without_changing_bindings_or_input_spans() {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let dependencies = catalog(&[("picked", "SELECT * FROM entries WHERE id = ${cte:int4}")]);
    let sql = "/* 日本語 */ SELECT *, ${base:int4} AS p FROM picked WHERE id = ${base}";
    let mut original = None;
    for enabled in [false, true] {
        let initialized = connection
            .rpc(
                "initialize_schema_context",
                &rewrite_request(&project, SCHEMA, Some(enabled)),
            )
            .await;
        let response = connection.rpc("analyze_query", &::serde_json::json!({"context_handle": initialized["result"]["context_handle"], "query": {"Text": sql}, "resolved_cte_catalog": dependencies})).await;
        let mut item = response["result"]["queries"]["items"][0].clone();
        assert_eq!(item["complete"], true, "{response}");
        let output = item["execute_sql"].as_str().unwrap();
        assert_eq!(
            output.matches('*').count(),
            if enabled { 1 } else { 2 },
            "{output}"
        );
        assert!(output.contains("$1") && output.contains("$2"), "{output}");
        item.as_object_mut().unwrap().remove("execute_sql");
        // Collections have intentionally distinct session-local issuance identities.
        for field in [
            "parameters",
            "result_columns",
            "probrems",
            "accessing_relations",
            "column_access",
        ] {
            item[field].as_object_mut().unwrap().remove("handle");
        }
        if let Some(original) = &original {
            assert_eq!(&item, original);
        } else {
            original = Some(item);
        }
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn reparse_keeps_original_multistatement_parameter_and_diagnostic_coordinates() {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let sql = "/* 日本語 */ SELECT *, ${first:int4} AS p FROM entries; SELECT missing, ${second:int4} FROM entries";
    let mut original = None;
    for enabled in [false, true] {
        let initialized = connection
            .rpc(
                "initialize_schema_context",
                &rewrite_request(&project, SCHEMA, Some(enabled)),
            )
            .await;
        let response = connection.rpc("analyze_query", &::serde_json::json!({"context_handle": initialized["result"]["context_handle"], "query": {"Text": sql}})).await;
        let mut items = response["result"]["queries"]["items"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(items.len(), 2, "{response}");
        assert_eq!(items[0]["complete"], true, "{response}");
        assert_eq!(items[1]["complete"], false, "{response}");
        for item in &mut items {
            item.as_object_mut().unwrap().remove("execute_sql");
            for field in [
                "parameters",
                "result_columns",
                "probrems",
                "accessing_relations",
                "column_access",
            ] {
                item[field].as_object_mut().unwrap().remove("handle");
            }
        }
        if let Some(original) = &original {
            assert_eq!(&items, original);
        } else {
            original = Some(items);
        }
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn plain_rewrite_preserves_parser_anchored_diagnostic_spans() {
    let project = query_project();
    // The RPC fixture verifies parser-retained spans as well as semantic diagnostics.
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let sql = "SELECT /* 日本語 and spacing */ *, identity(a => 1, a => 2) FROM entries";
    let mut original = None;
    for enabled in [false, true] {
        let initialized = connection
            .rpc(
                "initialize_schema_context",
                &rewrite_request(&project, SCHEMA, Some(enabled)),
            )
            .await;
        let response = connection.rpc(
            "analyze_query",
            &::serde_json::json!({"context_handle": initialized["result"]["context_handle"], "query": {"Text": sql}}),
        ).await;
        let item = &response["result"]["queries"]["items"][0];
        assert_eq!(item["complete"], false, "{response}");
        let problem = QueryProblems::from(item)
            .find("call_argument_duplicate_name")
            .expect("duplicate named argument diagnostic");
        assert_eq!(problem["presentation"]["source"], sql);
        assert_eq!(
            problem["statement_relative_span"]["offset"],
            sql.rfind("a =>").unwrap()
        );
        if let Some(original) = &original {
            assert_eq!(problem, original);
        } else {
            original = Some(problem.clone());
        }
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_output_shape_survives_schema_additions_and_unreferenced_drops() {
    use super::test_helpers::heavy;

    let item = heavy::analyze_schema_snapshot(SCHEMA, "SELECT * FROM entries", Some(true)).await;
    assert_eq!(item["complete"], true, "{item}");
    let explicit = item["execute_sql"].as_str().unwrap();
    let expected = &item["result_columns"]["items"];
    for schema in [
        "CREATE TABLE entries(id int4 NOT NULL, value int4, added int4);",
        "CREATE TABLE entries(id int4 NOT NULL, value int4);",
    ] {
        let item = heavy::analyze_schema_snapshot(schema, explicit, Some(false)).await;
        assert_eq!(item["complete"], true, "{item}");
        assert_eq!(&item["result_columns"]["items"], expected);
    }
    let item = heavy::analyze_schema_snapshot(
        "CREATE TABLE entries(id int4 NOT NULL);",
        explicit,
        Some(false),
    )
    .await;
    assert_eq!(item["complete"], false, "{item}");
    assert!(
        QueryProblems::from(&item)
            .find("column_not_found_in_table")
            .is_some(),
        "{item}"
    );
}

#[rstest::rstest]
#[case::duplicate_argument(
    "/* 日本語 */ SELECT *, identity(a => 1, a => 2) FROM picked",
    "call_argument_duplicate_name"
)]
#[case::missing_function(
    "/* 日本語 */ SELECT *, missing_wildcard_function(1) FROM picked",
    "SF00002"
)]
#[tokio::test(flavor = "multi_thread")]
async fn imported_cte_reparse_preserves_typed_diagnostic_coordinates(
    #[case] sql: &str,
    #[case] code: &str,
) {
    let project = query_project();
    let mut connection = heavy::Connection::start(16384, 2).await;
    connection.authenticate().await;
    let dependencies = catalog(&[("picked", "SELECT * FROM entries")]);
    let mut original = None;
    for enabled in [false, true] {
        let initialized = connection
            .rpc(
                "initialize_schema_context",
                &rewrite_request(&project, SCHEMA, Some(enabled)),
            )
            .await;
        let response = connection.rpc("analyze_query", &::serde_json::json!({"context_handle": initialized["result"]["context_handle"], "query": {"Text": sql}, "resolved_cte_catalog": dependencies})).await;
        let item = &response["result"]["queries"]["items"][0];
        assert_eq!(item["complete"], false, "{response}");
        let problem = QueryProblems::from(item)
            .find(code)
            .expect("requested typed diagnostic");
        if let Some(original) = &original {
            assert_eq!(problem, original);
        } else {
            original = Some(problem.clone());
        }
    }
    connection.end_requests().await;
    connection.finish().await.unwrap();
}
