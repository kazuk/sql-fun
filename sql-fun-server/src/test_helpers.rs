use crate::schema_cache::test_helpers::SchemaProject;
use ::sql_fun_server_api::payloads::InitializeSchemaArgs;

pub(crate) mod heavy;
pub(crate) mod lint_expectations;
pub(crate) mod pid_file;

/// Create independent cache state for a test-owned analysis base.
pub(crate) fn create_cache_context() -> ::sql_fun_sqlast::CacheContext {
    ::sql_fun_sqlast::CacheContext::new()
}

pub(crate) fn extension_script_with_control(control: &str) -> ::sql_fun_core::ExtensionScript {
    let directory = ::tempfile::tempdir().unwrap();
    let path = directory.path();
    std::fs::write(path.join("example.control"), control).unwrap();
    std::fs::write(path.join("example--1.0.sql"), "").unwrap();
    "example@1.0"
        .parse::<::sql_fun_core::PostgresExtensionsCollection>()
        .unwrap()
        .scripts(path)
        .unwrap()
        .pop()
        .unwrap()
        .pop()
        .unwrap()
}

/// Prepare datasource initialization for the selected PostgreSQL extensions.
pub(crate) fn extension_initialization_request(
    project: &SchemaProject,
    selected: &str,
    files: &[(&str, &str)],
) -> InitializeSchemaArgs {
    let directory = project.home().join("postgres/17/extensions");
    std::fs::create_dir_all(&directory).unwrap();
    for (file, text) in files {
        std::fs::write(directory.join(file), text).unwrap();
    }

    let mut request = project.rpc_request("-- unused");
    request["schema_level"] = ::serde_json::json!("extensions");
    request["environments"]["SQL_FUN_EXTENSIONS"] = ::serde_json::json!(selected);
    ::serde_json::from_value(request).unwrap()
}
