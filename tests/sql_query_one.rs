#[test]
fn test_sql_query_one() {
    if std::env::var_os("NEXTEST").is_none() {
        panic!("This test must be run with cargo-nextest (NEXTEST env var is not set).");
    }
    let test_name = std::env::var("NEXTEST_TEST_NAME")
        .unwrap_or_else(|_| "unknown_test".to_string())
        .replace("::", "/");
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("trybuild-nextest")
        .join(test_name);
    std::fs::create_dir_all(&dir).unwrap();
    let metadata_path = dir.join("Metadata.toml");

    std::fs::write(metadata_path, include_str!("./sql_query_one/Metadata.toml")).unwrap();

    #[expect(unsafe_code)]
    unsafe {
        std::env::set_var("SQL_FUN_METADATA_FILE", "Metadata.toml");
        std::env::set_var("CARGO_TARGET_DIR", &dir);
    }
    let t = trybuild::TestCases::new();
    t.pass("tests/sql_query_one/pass.rs");
}
