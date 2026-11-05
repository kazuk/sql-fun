use std::os::unix::fs::PermissionsExt;

/// A check fixture whose expanded query selects a configured non-default datasource.
pub(crate) fn expanded_named_query() -> ::tempfile::TempDir {
    let project = ::tempfile::tempdir().unwrap();
    let path = project.path();
    std::fs::write(
        path.join("Cargo.toml"),
        r#"[package.metadata.database.configured]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "tokio-postgres"
search_path = ["public"]
"#,
    )
    .unwrap();
    std::fs::write(path.join("sqlfun.develop.sql"), "-- schema\n").unwrap();
    // A bad existing lockfile demonstrates which extracted datasource check used.
    std::fs::write(path.join("configured.develop.lock"), "not a lockfile").unwrap();
    let catalog = ::serde_json::to_string(&::sql_fun_core::ResolvedCteCatalog::default()).unwrap();
    let codegen_manifest: ::toml::Value =
        ::toml::from_str(include_str!("../../../sql-fun-codegen/Cargo.toml")).unwrap();
    let version = codegen_manifest["package"]["version"].as_str().unwrap();
    let expanded = format!(
        r#"fn selected_query() {{
    const __SQL_FUN_CODEGEN_VERSION: &str = {version:?};
    const __SQL_FUN_SOURCE_FILE: &str = "src/lib.rs";
    const __SQL_FUN_SOURCE_LINE: u32 = 1;
    const __SQL_FUN_DATASOURCE: &str = "configured";
    const __SQL_FUN_QUERY: &str = "SELECT 1";
    const __SQL_FUN_USING_CTE: &str = {catalog:?};
}}"#
    );
    std::fs::write(path.join("expanded.rs"), expanded).unwrap();
    for (name, script) in [
        ("cargo", "#!/bin/sh\nexec /bin/cat expanded.rs\n"),
        ("cargo-expand", "#!/bin/sh\nexit 0\n"),
    ] {
        let executable = path.join(name);
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    project
}
