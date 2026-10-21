// Reuse the maintained real-server lifecycle and lightweight schema fixtures.
#[allow(dead_code)]
#[path = "../../../../../sql-fun-client/tests/test_helpers/heavy.rs"]
mod lifecycle;
#[allow(dead_code)]
#[path = "../../../../../sql-fun-client/tests/test_helpers/schema.rs"]
mod schema;

/// Starts a real server and loads a synthetic builtin catalog per test process.
/// Required to verify extracted requirements reach the actual realization service.
pub(crate) async fn analyze_recovered_catalog() -> ::testresult::TestResult {
    let server = lifecycle::ServerFixture::new()?;
    let schema = schema::SchemaFixture::with_query_schema("CREATE TABLE entries(id int4);")?;
    let request = schema.request();
    let args = ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())?
        .metadata_file(
            request["environments"]["SQL_FUN_METADATA_FILE"]
                .as_str()
                .unwrap(),
        )
        .sql_fun_home(request["environments"]["SQL_FUN_HOME"].as_str().unwrap())
        .schema_file(request["schema_dump"].as_str().unwrap())
        .build()?;
    let catalog = super::catalog();
    let mut client = ::sql_fun_client::connect(&server.options()).await?;
    let results = super::collect_submission(
        &mut client,
        &args,
        super::requirements(&["products"], &catalog),
    )
    .await?;
    assert_eq!(results.len(), 1);
    let item = &results[0].response.queries().items()[0];
    assert!(*item.complete());
    assert!(item.execute_sql().as_ref().unwrap().contains("WITH"));
    let parameters = item.parameters().items();
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name(), "shared");
    assert_eq!(*parameters[0].parameter_index(), 1);
    client.finish().await?;
    Ok(())
}
