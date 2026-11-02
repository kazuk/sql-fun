//! Generic setup dispatch and initialization followed by normal `PostgreSQL` bootstrap.
mod test_helpers;

use test_helpers::initialization::InitializedProject;

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
    project.assert_installed_catalog("17");
    assert!(!project.home().join("postgres/.17.init.lock").exists());
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
fn initialization_ignores_a_historical_lock_for_the_same_version() {
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
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    project.assert_installed_catalog("17");
    assert!(project.home().join("postgres/.17.init.lock").is_file());
}

#[test]
fn initialization_diagnoses_a_configured_datasource_version_outside_the_selection() {
    let project = InitializedProject::new();
    project.install_fixture("17");
    std::fs::write(project.directory().join("Cargo.toml"), "[package.metadata.database.default]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '18'\n").unwrap();
    let result = project
        .command()
        .args([
            "init",
            "--datasource",
            "default",
            "--yes",
            "--install-default-theme",
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
    project.assert_installed_catalog("17");
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
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
            "./relative-home",
        ])
        .output()
        .unwrap();
    let home = project.directory().join("relative-home");
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
            "--install-default-theme",
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
        .env("HOME", project.directory())
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
            "~/expanded-home",
        ])
        .output()
        .unwrap();
    let home = project.directory().join("expanded-home");
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
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
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
    project.assert_installed_catalog("17");
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
    test_helpers::initialization::retain_theme(&project.home());
    let result = project
        .command()
        .env("PATH", project.directory())
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--datasource",
            "local",
            "--sql-fun-home",
        ])
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
        .args(["init", "--yes", "--install-default-theme", "--sql-fun-home"])
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
    test_helpers::initialization::retain_theme(&project.home());
    let mut command = project.command();
    if ambient_metadata {
        command.env(
            "SQL_FUN_METADATA_FILE",
            project.directory().join("Cargo.toml"),
        );
    }
    let result = command
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .env("PATH", project.directory().join("no-tools"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("still under development"));
}

#[rstest::rstest]
#[case::collations("collations.json")]
#[case::operator_classes("operator_classes.json")]
fn incomplete_master_generation_is_rejected(#[case] artifact: &str) {
    let project = InitializedProject::new();
    project.install_fixture("17");
    std::fs::remove_file(project.home().join("postgres/17").join(artifact)).unwrap();
    let result = project.init(&["--engine-versions", "17"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains(artifact));
}

#[test]
fn a_missing_theme_requires_explicit_download_authorization() {
    let project = InitializedProject::new();
    let output = project
        .command()
        .args(["init", "--yes", "--engines", "sqlite", "--sql-fun-home"])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--install-default-theme"));
    assert!(!project.home().exists());
    assert!(!project.directory().join("downloads").exists());
}

#[test]
fn downloaded_theme_is_retained_without_downloader_on_rerun() {
    let project = InitializedProject::new();
    let output = project.init_scope(&["--engines", "sqlite"], &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let theme = project.home().join("highlighter/default.toml");
    let customized = "keyword = { foreground = 'magenta' }";
    std::fs::write(&theme, customized).unwrap();
    let before = std::fs::read(project.directory().join("downloads")).unwrap();
    let output = project
        .command()
        .args(["init", "--yes", "--engines", "sqlite", "--sql-fun-home"])
        .arg(project.home())
        .env("PATH", project.directory().join("missing-tools"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(theme).unwrap(), customized);
    assert_eq!(
        std::fs::read(project.directory().join("downloads")).unwrap(),
        before
    );
}

#[test]
fn uses_wget_when_curl_is_absent() {
    let project = InitializedProject::new();
    let mut command = project.command();
    let tools = project.directory().join("theme-tools");
    std::fs::rename(tools.join("curl"), tools.join("wget")).unwrap();
    let output = command
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        std::fs::read_to_string(project.directory().join("downloads"))
            .unwrap()
            .contains("--output-document")
    );
}

#[test]
fn reports_missing_download_tools_without_claiming_success() {
    let project = InitializedProject::new();
    let output = project
        .command()
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .env("PATH", project.directory().join("missing-tools"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires curl or wget"));
    assert!(!project.home().join("highlighter/default.toml").exists());
}

#[test]
fn curl_failure_never_falls_back_or_claims_success() {
    let project = InitializedProject::new();
    let mut command = project.command();
    test_helpers::initialization::reject_curl_download(project.directory());
    let output = command
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("theme download from"));
    assert!(
        !project.directory().join("downloads").exists(),
        "wget fallback must not run"
    );
    assert!(!project.home().join("highlighter/default.toml").exists());
}

#[test]
fn invalid_download_is_never_published() {
    let project = InitializedProject::new();
    let invalid = project.directory().join("invalid.toml");
    std::fs::write(&invalid, "keyword = { foreground = 'invalid-color' }").unwrap();
    let output = project
        .command()
        .args([
            "init",
            "--yes",
            "--install-default-theme",
            "--engines",
            "sqlite",
            "--sql-fun-home",
        ])
        .arg(project.home())
        .env("SQL_FUN_TEST_THEME", invalid)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("downloaded/edited theme is invalid"));
    assert!(!project.home().join("highlighter/default.toml").exists());
    assert_eq!(
        std::fs::read_dir(project.home().join("highlighter"))
            .unwrap()
            .count(),
        0
    );
}
