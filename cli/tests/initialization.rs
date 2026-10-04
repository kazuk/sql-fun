//! Initialization followed by loading through the normal PostgreSQL bootstrap path.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use ::sql_fun_core::PostgresVersionInventory;
use ::sql_fun_sqlast::{
    PostgresDialect,
    sem::{BaseContext, BaseParseContext, FullName, TableName},
};

struct InitializedProject {
    directory: tempfile::TempDir,
}
impl InitializedProject {
    fn new() -> Self {
        Self {
            directory: tempfile::tempdir().unwrap(),
        }
    }
    fn home(&self) -> PathBuf {
        self.directory.path().join("home")
    }
    fn command(&self) -> Command {
        let executable = std::env::var_os("SQL_FUN_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_sql-fun").into());
        let mut command = Command::new(executable);
        command
            .current_dir(self.directory.path())
            .env_remove("SQL_FUN_DATABASE_URL")
            .env_remove("SQL_FUN_HOME");
        command
    }
    fn init(&self, extra: &[&str]) -> std::process::Output {
        self.command()
            .args(["init", "--yes", "--sql-fun-home"])
            .arg(self.home())
            .args(extra)
            .output()
            .unwrap()
    }
    fn init_with_environment_home(&self) -> std::process::Output {
        self.command()
            .env("SQL_FUN_HOME", self.home())
            .args(["init", "--yes", "--engine-versions", "17"])
            .output()
            .unwrap()
    }
    fn init_for_global_version(&self, version: &str) -> std::process::Output {
        self.command()
            .args([
                "--postgres-version",
                version,
                "init",
                "--yes",
                "--sql-fun-home",
            ])
            .arg(self.home())
            .args(["--engine-versions", "17"])
            .output()
            .unwrap()
    }
    fn init_with_missing_metadata(&self) -> std::process::Output {
        self.command()
            .arg("--metadata-file")
            .arg(self.directory.path().join("missing.toml"))
            .args(["init", "--yes", "--sql-fun-home"])
            .arg(self.home())
            .args(["--engine-versions", "17"])
            .output()
            .unwrap()
    }
    fn install_fixture(&self, version: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sql-fun-home/postgres")
            .join(version);
        let target = self.home().join("postgres").join(version);
        std::fs::create_dir_all(&target).unwrap();
        for name in [
            "metadata.json",
            "schema.sql",
            "pg_type.json",
            "functions.json",
            "func_detail.json",
            "binary_op.json",
            "left_unary_op.json",
            "right_unary_op.json",
            "casts.json",
        ] {
            std::fs::copy(source.join(name), target.join(name)).unwrap();
        }
    }
    fn assert_semantics(&self, version: &str) {
        let metadata = self.directory.path().join("Cargo.toml");
        std::fs::write(&metadata, format!("[package.metadata.database.default]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '{version}'\n")).unwrap();
        let args = sql_fun_core::SqlFunArgsBuilder::default()
            .metadata_file(metadata.to_str().unwrap())
            .sql_fun_home(self.home())
            .build()
            .unwrap();
        let metadata = args.load_metadata().unwrap();
        let context = BaseContext::new(&args, metadata.get("default").unwrap()).unwrap();
        let builtins = PostgresDialect::parse_schema_file(
            &self.home().join(format!("postgres/{version}/schema.sql")),
            context,
        )
        .unwrap();
        let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
        assert!(builtins.get_table(&pg_class).is_some());
        let user = self.directory.path().join("schema.develop.sql");
        let sql =
            "CREATE TABLE hoge (id int PRIMARY KEY); SELECT abs(id) + 1 AS next_id FROM hoge;";
        std::fs::write(&user, sql).unwrap();
        let parsed = PostgresDialect::parse_schema_file(&user, builtins).unwrap();
        assert!(
            parsed
                .get_table(&TableName::from(FullName::with_schema("public", "hoge")))
                .is_some()
        );
        assert_eq!(std::fs::read_to_string(user).unwrap(), sql);
    }
}

#[test]
fn initialization_preserves_and_loads_an_existing_valid_generation() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init(&["--engine-versions", "17"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    project.assert_semantics("17");
}

#[test]
fn initialization_rejects_incomplete_existing_metadata() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let file = project.home().join("postgres/17/pg_type.json");
    std::fs::write(&file, "incomplete").unwrap();
    let result = project.init(&["--engine-versions", "17"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("incomplete or incompatible"));
    assert_eq!(std::fs::read_to_string(file).unwrap(), "incomplete");
}

#[test]
#[ignore = "requires Docker CLI/daemon and PostgreSQL client tools"]
fn docker_initialization_installs_all_supported_versions_for_semantic_analysis() {
    let project = InitializedProject::new();
    let host = std::env::var("SQL_FUN_TEST_DOCKER_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let bind = std::env::var("SQL_FUN_TEST_DOCKER_BIND").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let result = project.init(&[
        "--docker",
        "--docker-host-address",
        &host,
        "--docker-bind-address",
        &bind,
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for version in PostgresVersionInventory::builtin().stable_targets() {
        project.assert_semantics(&version.to_string());
    }
}

#[test]
fn initialization_rejects_a_concurrent_writer_for_the_same_version() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(project.home().join("postgres/.17.init.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let result = project.init(&["--engine-versions", "17"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("another initialization owns"));
}

#[test]
fn initialization_diagnoses_a_configured_datasource_version_outside_the_selection() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    std::fs::write(project.directory.path().join("Cargo.toml"), "[package.metadata.database.default]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '18'\n").unwrap();
    let result = project.init(&["--engine-versions", "17"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("configured PostgreSQL 18"));
}

#[test]
fn a_later_version_failure_preserves_the_completed_version() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    project.install_fixture("18");
    let metadata = project.home().join("postgres/17/metadata.json");
    let before = std::fs::read(&metadata).unwrap();
    std::fs::remove_file(project.home().join("postgres/18/casts.json")).unwrap();
    let result = project.init(&[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("failed for PostgreSQL 18"));
    assert_eq!(std::fs::read(metadata).unwrap(), before);
    project.assert_semantics("17");
}

#[test]
fn initialization_uses_the_same_environment_home_as_analysis() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init_with_environment_home();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("already initialized"));
    project.assert_semantics("17");
}

#[test]
fn explicit_version_is_validated_without_a_project() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init_for_global_version("18");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("configured PostgreSQL 18"));
}

#[test]
fn explicitly_configured_missing_metadata_is_an_error() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init_with_missing_metadata();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("missing.toml"));
}
