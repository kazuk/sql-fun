//! Command-specific prerequisite diagnostics without changing the parent PATH.

use std::process::Command;

#[test]
fn check_reports_missing_expand_before_reading_project_metadata() {
    let isolated = ::tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sql-fun"))
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
        let output = Command::new(env!("CARGO_BIN_EXE_sql-fun"))
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
        let output = Command::new(env!("CARGO_BIN_EXE_sql-fun"))
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
        let output = Command::new(env!("CARGO_BIN_EXE_sql-fun"))
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

    mod test_helpers {
        use std::os::unix::fs::PermissionsExt;

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
