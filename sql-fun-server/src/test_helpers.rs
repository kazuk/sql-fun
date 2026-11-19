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
}
