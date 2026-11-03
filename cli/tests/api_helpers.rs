#[path = "test_helpers.rs"]
pub mod executable;

#[path = "api_helpers/heavy.rs"]
pub mod heavy;
#[allow(dead_code)]
#[path = "../../sql-fun-client/tests/test_helpers/schema.rs"]
pub mod schema;

pub(crate) use self::executable::cli_executable;

pub fn responses(output: &std::process::Output) -> Vec<::serde_json::Value> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| ::serde_json::from_str(line).expect("complete JSON response line"))
        .collect()
}

/// Change the fixture's explicitly selected source after server analysis.
pub fn overwrite_schema_source(
    schema: &schema::SchemaFixture,
    sql: &str,
) -> ::testresult::TestResult {
    let path = schema.request()["schema_dump"]
        .as_str()
        .expect("fixture schema path");
    std::fs::write(path, sql)?;
    Ok(())
}
