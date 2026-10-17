//! Generic setup dispatch and initialization followed by normal `PostgreSQL` bootstrap.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

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
        let mut command = Command::new(env!("CARGO_BIN_EXE_sql-fun"));
        command
            .current_dir(self.directory.path())
            .env_remove("SQL_FUN_DATABASE_URL")
            .env_remove("SQL_FUN_HOME")
            .env_remove("SQL_FUN_DATASOURCE")
            .env_remove("SQL_FUN_METADATA_FILE");
        command
    }
    fn init(&self, extra: &[&str]) -> std::process::Output {
        self.command()
            .args([
                "init",
                "--yes",
                "--engines",
                "postgres",
                "--docker",
                "--sql-fun-home",
            ])
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
            .args([
                "--engines",
                "postgres",
                "--docker",
                "--engine-versions",
                "17",
            ])
            .output()
            .unwrap()
    }
    fn init_with_missing_metadata(&self) -> std::process::Output {
        self.command()
            .arg("--metadata-file")
            .arg(self.directory.path().join("missing.toml"))
            .args([
                "init",
                "--yes",
                "--engines",
                "postgres",
                "--docker",
                "--sql-fun-home",
            ])
            .arg(self.home())
            .args(["--engine-versions", "17"])
            .output()
            .unwrap()
    }
    fn write_project_metadata(&self, declarations: &str) {
        std::fs::write(self.directory.path().join("Cargo.toml"), declarations).unwrap();
    }
    fn datasource_without_engine(&self) {
        std::fs::write(
            self.directory.path().join("Cargo.toml"),
            "[package.metadata.database.only]\ndialect = 'postgres'\nversion = '17'\n",
        )
        .unwrap();
    }
    fn mixed_project(&self) {
        std::fs::write(self.directory.path().join("Cargo.toml"), "[package.metadata.database.pg]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'\n[package.metadata.database.local]\nengine = 'sqlite'\ndialect = 'sqlite'\n[package.metadata.database.other]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '18'\n").unwrap();
    }
    fn sqlite_project(&self) {
        std::fs::write(
            self.directory.path().join("Cargo.toml"),
            "[package.metadata.database.only]\nengine = 'sqlite'\ndialect = 'sqlite'\n",
        )
        .unwrap();
    }
    fn init_scope(&self, scope: &[&str], options: &[&str]) -> std::process::Output {
        self.command()
            .arg("init")
            .args(scope)
            .args(["--yes", "--sql-fun-home"])
            .arg(self.home())
            .args(options)
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
        let builtins = {
            let analysis_base = &::sql_fun_sqlast::sem::FrozenBaseContext::from(context);
            PostgresDialect::parse_schema_file(
                &self.home().join(format!("postgres/{version}/schema.sql")),
                analysis_base,
                &mut None,
                &args,
                metadata.get("default").unwrap(),
            )
        }
        .unwrap();
        let pg_class = TableName::from(FullName::with_schema("pg_catalog", "pg_class"));
        assert!(builtins.get_table(&pg_class).is_some());
        let user = self.directory.path().join("schema.develop.sql");
        let sql =
            "CREATE TABLE hoge (id int PRIMARY KEY); SELECT abs(id) + 1 AS next_id FROM hoge;";
        std::fs::write(&user, sql).unwrap();
        let parsed = {
            let analysis_base = builtins.base_context();
            PostgresDialect::parse_schema_file(
                &user,
                analysis_base,
                &mut None,
                &args,
                metadata.get("default").unwrap(),
            )
        }
        .unwrap();
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
    let result = project
        .command()
        .args([
            "init",
            "--datasource",
            "default",
            "--yes",
            "--docker",
            "--engine-versions",
            "17",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .output()
        .unwrap();
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
    let result = project.init(&["--engine-versions", "17,18"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("failed for PostgreSQL 18"));
    assert_eq!(std::fs::read(metadata).unwrap(), before);
    project.assert_semantics("17");
}

#[test]
fn environment_home_is_not_consent_to_noninteractive_setup() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init_with_environment_home();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--sql-fun-home"));
}

#[test]
fn noninteractive_relative_home_uses_command_working_directory() {
    let project = InitializedProject::new();
    let result = project
        .command()
        .args([
            "init",
            "--yes",
            "--engines",
            "sqlite",
            "--sql-fun-home",
            "./relative-home",
        ])
        .output()
        .unwrap();
    let home = project.directory.path().join("relative-home");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(home.is_dir());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(&format!("Created {}", home.display()))
    );
    let marker = home.join("marker");
    std::fs::write(&marker, "keep").unwrap();
    let rerun = project
        .command()
        .args([
            "init",
            "--yes",
            "--engines",
            "sqlite",
            "--sql-fun-home",
            "./relative-home",
        ])
        .output()
        .unwrap();
    assert!(rerun.status.success());
    assert!(
        String::from_utf8_lossy(&rerun.stderr).contains(&format!("Reusing {}", home.display()))
    );
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "keep");
}

#[test]
fn noninteractive_home_relative_option_expands_home_directory() {
    let project = InitializedProject::new();
    let result = project
        .command()
        .env("HOME", project.directory.path())
        .args([
            "init",
            "--yes",
            "--engines",
            "sqlite",
            "--sql-fun-home",
            "~/expanded-home",
        ])
        .output()
        .unwrap();
    let home = project.directory.path().join("expanded-home");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(home.is_dir());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(&format!("Created {}", home.display()))
    );
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

#[rstest::rstest]
#[case::explicit_engine(&[], &["--engines", "sqlite"])]
#[case::explicit_datasource(&["--datasource", "only"], &[])]
fn sqlite_dispatch_never_runs_postgres_setup(#[case] scope: &[&str], #[case] options: &[&str]) {
    let project = InitializedProject::new();
    project.sqlite_project();
    let result = project.init_scope(scope, options);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("SQLite development-support setup is still under development")
    );
    assert!(!project.home().join("postgres").exists());
}

#[rstest::rstest]
#[case::missing_scope(&[], "--engines or --datasource")]
#[case::missing_source(&["--engines", "postgres", "--engine-versions", "17"], "--docker or --database-url")]
#[case::missing_version(&["--engines", "postgres", "--docker"], "--engine-versions")]
#[case::postgres_options_with_sqlite(&["--engines", "sqlite", "--docker"], "PostgreSQL options require")]
fn noninteractive_omissions_fail_before_setup(#[case] options: &[&str], #[case] diagnostic: &str) {
    let project = InitializedProject::new();
    project.sqlite_project(); // A single configured datasource is still not a choice.
    let result = project.init_scope(&[], options);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains(diagnostic));
    assert!(!project.home().exists());
}

#[test]
fn ambient_postgres_connection_does_not_select_sqlite_setup_source() {
    let project = InitializedProject::new();
    let result = project
        .command()
        .env("SQL_FUN_DATABASE_URL", "host=must-not-connect.invalid")
        .args(["init", "--yes", "--engines", "sqlite", "--sql-fun-home"])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!project.home().join("postgres").exists());
}

#[test]
fn explicit_mixed_engine_setup_retains_both_dispatch_paths() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    let result = project.init_scope(
        &[],
        &[
            "--engines",
            "postgres,sqlite",
            "--docker",
            "--engine-versions",
            "17",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("SQLite development-support setup is still under development")
    );
    project.assert_semantics("17");
}

#[test]
fn selected_datasources_do_not_select_other_project_tasks() {
    let project = InitializedProject::new();
    project.mixed_project();
    project.install_fixture("17");
    let result = project.init_scope(
        &["--datasource", "pg,local"],
        &["--docker", "--engine-versions", "17"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("SQLite development-support setup is still under development")
    );
    assert!(!project.home().join("postgres/18").exists());
}

#[test]
fn sqlite_only_datasource_requires_no_postgres_runtime_tools() {
    let project = InitializedProject::new();
    project.mixed_project();
    let result = project
        .command()
        .env("PATH", project.directory.path())
        .args(["init", "--yes", "--datasource", "local", "--sql-fun-home"])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!project.home().join("postgres").exists());
}

#[test]
fn a_datasource_dialect_is_not_an_implicit_engine_choice() {
    let project = InitializedProject::new();
    project.datasource_without_engine();
    let result = project.init_scope(
        &["--datasource", "only"],
        &["--docker", "--engine-versions", "17"],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("engine not specified"));
    assert!(!project.home().exists());
}

#[test]
fn an_ambient_datasource_is_not_a_noninteractive_cli_selection() {
    let project = InitializedProject::new();
    project.sqlite_project();
    let result = project
        .command()
        .env("SQL_FUN_DATASOURCE", "only")
        .args(["init", "--yes", "--sql-fun-home"])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--engines or --datasource"));
    assert!(!project.home().exists());
}

#[rstest::rstest]
#[case::named(
    "[package.metadata.database.private]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'\n"
)]
#[case::malformed("not valid TOML [")]
fn engine_setup_ignores_unrequested_project_metadata(
    #[case] declarations: &str,
    #[values(false, true)] ambient_metadata: bool,
) {
    let project = InitializedProject::new();
    project.write_project_metadata(declarations);
    let mut command = project.command();
    if ambient_metadata {
        command.env(
            "SQL_FUN_METADATA_FILE",
            project.directory.path().join("Cargo.toml"),
        );
    }
    let result = command
        .args(["init", "--yes", "--engines", "sqlite", "--sql-fun-home"])
        .arg(project.home())
        .env("PATH", project.directory.path().join("no-tools"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("still under development"));
}
