use std::path::{Path, PathBuf};

pub(super) struct ExtensionFixture {
    directory: ::tempfile::TempDir,
}

impl ExtensionFixture {
    pub(super) fn new(
        name: &str,
        versions: &[&str],
        control: Option<&str>,
    ) -> std::io::Result<Self> {
        let fixture = Self {
            directory: ::tempfile::tempdir()?,
        };
        for version in versions {
            std::fs::write(fixture.script(name, version), "-- fixture")?;
        }
        if let Some(control) = control {
            std::fs::write(fixture.path().join(format!("{name}.control")), control)?;
        }
        Ok(fixture)
    }

    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn script(&self, name: &str, version: &str) -> PathBuf {
        self.path().join(format!("{name}--{version}.sql"))
    }

    pub(super) fn script_directory(&self, name: &str, version: &str) -> std::io::Result<()> {
        std::fs::create_dir(self.script(name, version))
    }

    pub(super) fn unreadable_control(&self, name: &str) -> std::io::Result<()> {
        std::fs::create_dir(self.path().join(format!("{name}.control")))
    }

    pub(super) fn uuid_ossp() -> std::io::Result<Self> {
        // PostgreSQL 17.6 contrib/uuid-ossp/uuid-ossp.control and script names.
        Self::new(
            "uuid-ossp",
            &["1.1", "1.0--1.1"],
            Some(
                "# uuid-ossp extension\ncomment = 'generate universally unique identifiers (UUIDs)'\ndefault_version = '1.1'\nmodule_pathname = '$libdir/uuid-ossp'\nrelocatable = true\ntrusted = true\n",
            ),
        )
    }
}
