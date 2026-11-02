use std::{
    fs,
    path::{Path, PathBuf},
};

/// A private home and explicitly selected local programs; never contacts hosting.
pub(crate) struct ThemeFixture(tempfile::TempDir);
impl ThemeFixture {
    pub(crate) fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    pub(crate) fn home(&self) -> &Path {
        self.0.path()
    }
    pub(crate) fn theme(&self) -> PathBuf {
        self.home().join("highlighter/default.toml")
    }
    pub(crate) fn write_theme(&self, text: &str) {
        fs::create_dir_all(self.home().join("highlighter")).unwrap();
        fs::write(self.theme(), text).unwrap();
    }
    pub(crate) fn program(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let program = self.home().join(name);
        fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        program
    }
    pub(crate) fn assert_theme(&self, text: &str) {
        assert_eq!(fs::read_to_string(self.theme()).unwrap(), text);
    }
    pub(crate) fn assert_no_stage(&self) {
        assert_eq!(
            fs::read_dir(self.home().join("highlighter"))
                .unwrap()
                .count(),
            1
        );
    }
}
