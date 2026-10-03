//! rusqlite compilation test

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ::scopeguard::ScopeGuard;

use self::test_helpers::parenthesized_alias_database;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[rstest::rstest]
#[case("select q.ax from (a) as q", true)]
#[case("select a.ax from (a) as q", false)]
#[case("select q.ax from ((a)) as q", true)]
#[case("select r.ax from ((a) as q) as r", true)]
#[case("select q.ax from ((a) as q) as r", false)]
#[case("select q.ax, q.bx from (a join b) as q", true)]
#[case("select a.ax, b.bx from (a join b) as q", true)]
#[case("select q.bx from (a, b) as q", true)]
#[case("select q.cx from (a join b join c) as q", true)]
#[case("select q.bx from ((a join b)) as q", true)]
#[case("select q.id from (a join b) as q", true)]
#[case("select q.id from (a join b using(id)) as q", true)]
#[case("select q.absent from (a join b) as q", false)]
#[case("select id from (a join b) as q", false)]
#[case("select q.* from (a join b) as q", false)]
fn bundled_sqlite_parenthesized_alias_visibility(
    #[case] sql: &str,
    #[case] valid: bool,
) -> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    assert_eq!(database.prepare(sql).is_ok(), valid, "{sql}");
    Ok(())
}

#[test]
fn bundled_sqlite_group_alias_uses_first_matching_column_and_preserves_outer_join()
-> Result<(), ::rusqlite::Error> {
    let database = parenthesized_alias_database()?;
    let id: i64 = database.query_row("select q.id from (a join b) as q", [], |row| row.get(0))?;
    assert_eq!(id, 1);
    let right: Option<String> = database.query_row(
        "select q.bx from (a left join b on false) as q",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(right, None);
    Ok(())
}

mod test_helpers {
    pub(super) fn parenthesized_alias_database() -> Result<::rusqlite::Connection, ::rusqlite::Error>
    {
        let database = ::rusqlite::Connection::open_in_memory()?;
        database.execute_batch(
            "create table a(id integer not null, ax integer not null);
             create table b(id text not null, bx text not null);
             create table c(id text not null, cx text not null);
             insert into a values(1, 7);
             insert into b values('1', 'right');
             insert into c values('1', 'third');",
        )?;
        Ok(database)
    }
}

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
    let _working_dir_cleanup = setup_working_dir_cleanup(&dir);
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

#[expect(
    clippy::manual_assert,
    reason = "Preserve the existing explicit nextest environment guard in this focused fix."
)]
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

fn setup_working_dir_cleanup(dir: &Path) -> ScopeGuard<PathBuf, impl FnOnce(PathBuf)> {
    ::scopeguard::guard(dir.to_path_buf(), |path| {
        std::fs::remove_dir_all(path).unwrap();
    })
}

#[expect(
    clippy::ptr_arg,
    reason = "Preserve the existing fixture helper interface in this codegen fix."
)]
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

#[expect(
    clippy::ptr_arg,
    reason = "Preserve the existing fixture helper interface in this codegen fix."
)]
fn setup_file<P: AsRef<Path>>(dir: &PathBuf, file_name: P, content: &str) {
    std::fs::write(dir.join(file_name), content).unwrap();
}
