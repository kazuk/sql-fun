use crate::test_helpers::cli_executable;

use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct InitializedProject {
    directory: tempfile::TempDir,
}
impl InitializedProject {
    pub(crate) fn new() -> Self {
        Self {
            directory: tempfile::tempdir().unwrap(),
        }
    }
    pub(crate) fn directory(&self) -> &Path {
        self.directory.path()
    }
    pub(crate) fn home(&self) -> PathBuf {
        self.directory.path().join("home")
    }
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(cli_executable());
        let bin = theme_downloader(self.directory.path());
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sql-fun-home/highlighter/default.toml");
        command
            .env("PATH", bin)
            .env("SQL_FUN_TEST_THEME", source)
            .env(
                "SQL_FUN_TEST_TRACE",
                self.directory.path().join("downloads"),
            );
        command
            .current_dir(self.directory.path())
            .env_remove("SQL_FUN_DATABASE_URL")
            .env_remove("SQL_FUN_HOME")
            .env_remove("SQL_FUN_DATASOURCE")
            .env_remove("SQL_FUN_METADATA_FILE");
        command
    }
    pub(crate) fn init(&self, extra: &[&str]) -> std::process::Output {
        self.command()
            .args([
                "init",
                "--yes",
                "--install-default-theme",
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
    pub(crate) fn init_with_environment_home(&self) -> std::process::Output {
        self.command()
            .env("SQL_FUN_HOME", self.home())
            .args([
                "init",
                "--yes",
                "--install-default-theme",
                "--engine-versions",
                "17",
            ])
            .output()
            .unwrap()
    }
    pub(crate) fn init_for_global_version(&self, version: &str) -> std::process::Output {
        self.command()
            .args([
                "--postgres-version",
                version,
                "init",
                "--yes",
                "--install-default-theme",
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
    pub(crate) fn init_with_missing_metadata(&self) -> std::process::Output {
        self.command()
            .arg("--metadata-file")
            .arg(self.directory.path().join("missing.toml"))
            .args([
                "init",
                "--yes",
                "--install-default-theme",
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
    pub(crate) fn write_project_metadata(&self, declarations: &str) {
        std::fs::write(self.directory.path().join("Cargo.toml"), declarations).unwrap();
    }
    pub(crate) fn datasource_without_engine(&self) {
        std::fs::write(
            self.directory.path().join("Cargo.toml"),
            "[package.metadata.database.only]\ndialect = 'postgres'\nversion = '17'\n",
        )
        .unwrap();
    }
    pub(crate) fn mixed_project(&self) {
        std::fs::write(self.directory.path().join("Cargo.toml"), "[package.metadata.database.pg]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '17'\n[package.metadata.database.local]\nengine = 'sqlite'\ndialect = 'sqlite'\n[package.metadata.database.other]\nengine = 'PostgreSQL'\ndialect = 'postgres'\nversion = '18'\n").unwrap();
    }
    pub(crate) fn sqlite_project(&self) {
        std::fs::write(
            self.directory.path().join("Cargo.toml"),
            "[package.metadata.database.only]\nengine = 'sqlite'\ndialect = 'sqlite'\n",
        )
        .unwrap();
    }
    pub(crate) fn init_scope(&self, scope: &[&str], options: &[&str]) -> std::process::Output {
        self.command()
            .arg("init")
            .args(scope)
            .args(["--yes", "--install-default-theme", "--sql-fun-home"])
            .arg(self.home())
            .args(options)
            .output()
            .unwrap()
    }
    pub(crate) fn install_fixture(&self, version: &str) {
        retain_theme(&self.home());
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
            "collations.json",
            "operator_classes.json",
        ] {
            std::fs::copy(source.join(name), target.join(name)).unwrap();
        }
    }
    pub(crate) fn assert_installed_catalog(&self, version: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sql-fun-home/postgres")
            .join(version);
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
            "collations.json",
            "operator_classes.json",
        ] {
            assert_eq!(
                std::fs::read(self.home().join("postgres").join(version).join(name)).unwrap(),
                std::fs::read(source.join(name)).unwrap(),
                "installed {name}"
            );
        }
    }
}

/// An offline default-theme downloader for init's process boundary.
pub fn theme_downloader(directory: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = directory.join("theme-tools");
    std::fs::create_dir_all(&bin).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sql-fun-home/highlighter/default.toml");
    let program = bin.join("curl");
    std::fs::write(
        &program,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$SQL_FUN_TEST_TRACE"
while [ "$1" != --output ] && [ "$1" != --output-document ]; do shift; done
if [ "$1" = --output-document ]; then
    printf '  HTTP/1.1 200 OK\n' >&2
else
    printf 200
fi
shift
/bin/cp "$SQL_FUN_TEST_THEME" "$1"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(source.is_file());
    bin
}

/// Reuse the repository's completed-home theme for catalog-only init scenarios.
pub fn retain_theme(home: &std::path::Path) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sql-fun-home/highlighter/default.toml");
    std::fs::create_dir_all(home.join("highlighter")).unwrap();
    std::fs::copy(source, home.join("highlighter/default.toml")).unwrap();
}

/// An explicitly failing curl and a usable wget to detect forbidden fallback.
pub fn reject_curl_download(directory: &std::path::Path) {
    let tools = directory.join("theme-tools");
    std::fs::copy(tools.join("curl"), tools.join("wget")).unwrap();
    std::fs::write(tools.join("curl"), "#!/bin/sh\nexit 22\n").unwrap();
}
