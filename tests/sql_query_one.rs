//! compilation test

use std::path::{Path, PathBuf};

use scopeguard::ScopeGuard;

/// test [`sql_fun::sql_query_one`]
#[test]
fn test_sql_query_one() {
    validate_test_env();

    let dir = setup_working_dir();
    setup_metadata(&dir);
    setup_lockfile(&dir);
    setup_schema_dump(&dir);
    let logfile = dir.join("sql-fun.log");
    let _trace_dump = setup_trace_dump(&logfile);
    setup_env(&dir, &logfile, false);
    let t = trybuild::TestCases::new();
    t.pass("tests/sql_query_one/pass.rs");
}

/// test [`sql_fun::sql_query_one`] for SQL_FUN_MODE = CHECK
#[test]
fn test_sql_query_one_check() {
    validate_test_env();

    let dir = setup_working_dir();
    setup_metadata(&dir);
    setup_schema_dump(&dir);
    let logfile = dir.join("sql-fun.log");
    let _trace_dump = setup_trace_dump(&logfile);
    setup_env(&dir, &logfile, true);
    let t = trybuild::TestCases::new();
    t.pass("tests/sql_query_one/pass.rs");
}

fn validate_test_env() {
    if std::env::var_os("NEXTEST").is_none() {
        panic!("This test must be run with cargo-nextest (NEXTEST env var is not set).");
    }
}

fn setup_working_dir() -> PathBuf {
    let test_name = std::env::var("NEXTEST_TEST_NAME")
        .unwrap_or_else(|_| "unknown_test".to_string())
        .replace("::", "/");

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("trybuild-nextest")
        .join(test_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup_metadata(dir: &PathBuf) {
    let metadata_path = dir.join("Metadata.toml");
    std::fs::write(metadata_path, include_str!("./sql_query_one/Metadata.toml")).unwrap();
}

fn setup_lockfile(dir: &PathBuf) {
    setup_file(
        dir,
        "default.develop.lock",
        include_str!("./sql_query_one/default.develop.lock"),
    );
}

fn setup_schema_dump(dir: &PathBuf) {
    setup_file(
        dir,
        "default.develop.sql",
        include_str!("./sql_query_one/default.develop.sql"),
    );
}

fn setup_trace_dump(logfile: &PathBuf) -> ScopeGuard<PathBuf, impl FnOnce(PathBuf)> {
    let trace_dump = scopeguard::guard(logfile.clone(), |path| {
        if let Ok(trace) = std::fs::read_to_string(path) {
            if !trace.is_empty() {
                eprintln!("\nTRACE RECORDS");
                eprintln!("\n{trace}");
            }
        }
    });
    trace_dump
}

fn setup_env(dir: &PathBuf, logfile: &PathBuf, for_check: bool) {
    #[expect(unsafe_code)]
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
        std::env::set_var("SQL_FUN_METADATA_FILE", "Metadata.toml");
        std::env::set_var("CARGO_TARGET_DIR", &dir);
        std::env::set_var("RUST_LOG", "info");
        std::env::set_var("SQL_FUN_TRACE_FILE", logfile);
        if for_check {
            std::env::set_var("SQL_FUN_MODE", "CHECK")
        }
    }
}

fn setup_file<P: AsRef<Path>>(dir: &PathBuf, file_name: P, content: &str) {
    let schema_dump_path = dir.join(file_name);
    std::fs::write(schema_dump_path, content).unwrap();
}
