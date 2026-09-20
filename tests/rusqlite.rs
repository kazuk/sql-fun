//! rusqlite compilation test

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use scopeguard::ScopeGuard;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_rusqlite() {
    run_test(false);
}

#[test]
fn test_rusqlite_check() {
    run_test(true);
}

fn run_test(for_check: bool) {
    let _env_lock = ENV_LOCK.lock().unwrap();
    validate_test_env();

    let dir = setup_working_dir();
    setup_file(
        &dir,
        "Metadata.toml",
        include_str!("./rusqlite/Metadata.toml"),
    );
    setup_file(
        &dir,
        "sqlite.develop.sql",
        include_str!("./rusqlite/sqlite.develop.sql"),
    );
    if !for_check {
        setup_file(
            &dir,
            "sqlite.develop.lock",
            include_str!("./rusqlite/sqlite.develop.lock"),
        );
    }
    let logfile = dir.join("sql-fun.log");
    let _trace_dump = setup_trace_dump(&logfile);
    setup_env(&dir, &logfile, for_check);
    trybuild::TestCases::new().pass("tests/rusqlite/pass.rs");
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
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("trybuild-nextest")
        .join(test_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup_trace_dump(logfile: &PathBuf) -> ScopeGuard<PathBuf, impl FnOnce(PathBuf)> {
    scopeguard::guard(logfile.clone(), |path| {
        if let Ok(trace) = std::fs::read_to_string(path)
            && !trace.is_empty()
        {
            eprintln!("\nTRACE RECORDS\n\n{trace}");
        }
    })
}

fn setup_env(dir: &PathBuf, logfile: &PathBuf, for_check: bool) {
    #[expect(unsafe_code)]
    unsafe {
        std::env::set_var("SQL_FUN_METADATA_FILE", "Metadata.toml");
        std::env::set_var("CARGO_TARGET_DIR", dir);
        std::env::set_var("RUST_LOG", "info");
        std::env::set_var("SQL_FUN_TRACE_FILE", logfile);
        if for_check {
            std::env::set_var("SQL_FUN_MODE", "CHECK");
        } else {
            std::env::remove_var("SQL_FUN_MODE");
        }
    }
}

fn setup_file<P: AsRef<Path>>(dir: &PathBuf, file_name: P, content: &str) {
    std::fs::write(dir.join(file_name), content).unwrap();
}
