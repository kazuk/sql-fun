pub(super) mod heavy;

use crate::schema_cache::test_helpers::SchemaProject;

/// A small int4 catalog with the comparison used by the RPC scenarios.
pub(super) fn query_project() -> SchemaProject {
    let project = SchemaProject::new();
    let builtin = project.home().join("postgres/17");
    std::fs::write(
        builtin.join("pg_type.json"),
        r#"[{"name":"int4"},{"name":"bool"},{"name":"text"}]"#,
    )
    .unwrap();
    std::fs::write(
        builtin.join("binary_op.json"),
        r#"{
        "+":[{"left":"int4","right":"int4","result":"int4","strict":true}],
        "=":[{"left":"int4","right":"int4","result":"bool","strict":true}]
    }"#,
    )
    .unwrap();
    project
}

/// An explicitly selected query file in the fixture's own temporary project.
pub(super) fn query_file(project: &SchemaProject, sql: &str) -> std::path::PathBuf {
    let path = project.home().join("selected-query.sql");
    std::fs::write(&path, sql).unwrap();
    path
}

/// Check registration and release without duplicating the registry's paging tests.
pub(super) async fn verify_query_collection_lifecycle(
    connection: &mut crate::test_helpers::heavy::Connection,
    queries: &::serde_json::Value,
) {
    use ::serde_json::json;

    let items = queries["items"].as_array().unwrap();
    for collection in [
        queries,
        &items[0]["parameters"],
        &items[0]["result_columns"],
        &items[0]["probrems"],
        &items[0]["accessing_relations"],
        &items[0]["column_access"],
        &items[1]["parameters"],
    ] {
        let read =
            json!({"collection_handle":collection["handle"], "start_offset":0, "count":null});
        assert_eq!(
            connection.rpc("read_collection", &read).await["result"],
            *collection
        );
        assert_eq!(
            connection.rpc("read_collection", &read).await["result"],
            *collection
        );
        connection
            .rpc(
                "release_collection",
                &json!({"collection_handle":collection["handle"]}),
            )
            .await;
        assert_eq!(
            connection.rpc("read_collection", &read).await["error"]["data"],
            "InvalidCollectionHandle"
        );
    }
}

/// Select rewrite policy without changing the fixture's shared schema inputs.
pub(super) fn rewrite_request(
    project: &SchemaProject,
    schema: &str,
    enabled: Option<bool>,
) -> ::serde_json::Value {
    let path = project.home().join("Cargo.toml");
    let metadata = std::fs::read_to_string(&path).unwrap();
    let marker = "\n[package.metadata.database.products.rewrite_options]";
    let base = metadata
        .split_once(marker)
        .map_or(metadata.as_str(), |(base, _)| base);
    let options = enabled.map_or_else(String::new, |enabled| {
        format!("{marker}\nexpand_asterisk = {enabled}\n")
    });
    std::fs::write(path, format!("{base}{options}")).unwrap();
    project.rpc_request(schema)
}
