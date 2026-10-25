//! Command-specific prerequisite diagnostics without changing the parent PATH.

use std::process::Command;

#[test]
fn check_reports_missing_expand_before_reading_project_metadata() {
    let isolated = ::tempfile::tempdir().unwrap();
    let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
        .current_dir(isolated.path())
        .env("PATH", isolated.path())
        .arg("check")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("sql-fun check requires cargo-expand on PATH"),
        "{error}"
    );
    assert!(
        error.contains("cargo install --locked cargo-expand"),
        "{error}"
    );
    assert!(!error.contains("panicked"), "{error}");
}

#[test]
fn help_does_not_require_expand() {
    let isolated = ::tempfile::tempdir().unwrap();
    for args in [vec!["--help"], vec!["check", "--help"]] {
        let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
            .current_dir(isolated.path())
            .env("PATH", isolated.path())
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
}

#[cfg(unix)]
mod unix {
    use std::process::Command;

    #[test]
    fn non_executable_expand_is_reported_as_missing() {
        let tools = test_helpers::expand_tools(false);
        let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
            .current_dir(tools.path())
            .env("PATH", tools.path())
            .args(["check", "--package", "fixture"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("requires cargo-expand on PATH"), "{error}");
    }

    #[test]
    fn expansion_failure_is_not_reported_as_a_missing_prerequisite() {
        let tools = test_helpers::expand_tools(true);
        let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
            .current_dir(tools.path())
            .env("PATH", tools.path())
            .args(["check", "--package", "fixture"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("cargo-expand exited non-success"), "{error}");
        assert!(!error.contains("requires cargo-expand on PATH"), "{error}");
    }

    #[rstest::rstest]
    #[case::ordinary(&[])]
    #[case::validate_only(&["--validate-only"])]
    #[case::force(&["--force-rebuild"])]
    fn empty_extraction_warns_without_opening_lockfiles(#[case] options: &[&str]) {
        let tools = test_helpers::empty_query_tools();
        let sentinel = tools.path().join("selected.develop.lock");
        let before = std::fs::read(&sentinel).unwrap();
        let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
            .current_dir(tools.path())
            .env("PATH", tools.path())
            .env("SQL_FUN_CHECK_PROGRESS", "silent")
            .env("SQL_FUN_SERVER", "/nonexistent/server")
            .args(["--datasource", "selected", "check", "--package", "fixture"])
            .args(options)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("warning: no SQL queries found in the selected targets")
        );
        assert_eq!(std::fs::read(&sentinel).unwrap(), before);
    }

    #[rstest::rstest]
    #[case::default(None, None, true)]
    #[case::silent_env(Some("silent"), None, false)]
    #[case::names_env(Some("names"), None, true)]
    #[case::dots_env(Some("dots"), None, true)]
    #[case::cli_wins(Some("names"), Some("silent"), false)]
    #[case::cli_names(Some("silent"), Some("names"), true)]
    fn progress_configuration_uses_cli_over_environment(
        #[case] env: Option<&str>,
        #[case] cli: Option<&str>,
        #[case] visible: bool,
    ) {
        let tools = test_helpers::expand_tools(true);
        let mut command = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"));
        command
            .current_dir(tools.path())
            .env("PATH", tools.path())
            .env_remove("SQL_FUN_CHECK_PROGRESS")
            .args(["check", "--package", "fixture"]);
        if let Some(value) = env {
            command.env("SQL_FUN_CHECK_PROGRESS", value);
        }
        if let Some(value) = cli {
            command.args(["--progress", value]);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.contains("Extracting"), visible);
        assert_eq!(stdout.contains("Stopped"), visible);
        assert!(!stdout.contains("Completed"));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cargo-expand exited non-success")
        );
    }

    #[test]
    fn invalid_progress_is_an_argument_error() {
        let output = Command::new(std::env::var("SQL_FUN_BIN").expect("SQL_FUN_BIN must be set"))
            .env("SQL_FUN_CHECK_PROGRESS", "unknown")
            .arg("check")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value"));
    }

    mod test_helpers {
        use std::os::unix::fs::PermissionsExt;

        pub(super) fn empty_query_tools() -> ::tempfile::TempDir {
            let tools = expand_tools(true);
            std::fs::write(
                tools.path().join("sqlfun.develop.sql"),
                "-- extraction configuration\n",
            )
            .unwrap();
            std::fs::write(
                tools.path().join("cargo"),
                "#!/bin/sh\necho 'fn unrelated() {}'\n",
            )
            .unwrap();
            std::fs::write(
                tools.path().join("selected.develop.lock"),
                "malformed lockfile sentinel",
            )
            .unwrap();
            std::fs::write(
                tools.path().join("Cargo.toml"),
                r#"
[package.metadata.database.selected]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "tokio-postgres"
search_path = ["public"]
"#,
            )
            .unwrap();
            tools
        }

        pub(super) fn expand_tools(executable: bool) -> ::tempfile::TempDir {
            let tools = ::tempfile::tempdir().unwrap();
            std::fs::write(tools.path().join("Cargo.toml"), "[package.metadata]\n").unwrap();
            for name in ["cargo", "cargo-expand"] {
                let path = tools.path().join(name);
                std::fs::write(&path, "#!/bin/sh\nexit 17\n").unwrap();
                let mode = if name == "cargo-expand" && !executable {
                    0o644
                } else {
                    0o755
                };
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
            }
            tools
        }
    }
}
