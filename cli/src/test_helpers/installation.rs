use crate::init::install::{InstallationStage, REQUIRED, validate};

use std::{fs, path::PathBuf};

use ::sql_fun_core::PostgresVersion;

pub(crate) struct InstallationFixture {
    home: tempfile::TempDir,
    version: PostgresVersion,
}
impl InstallationFixture {
    pub(crate) fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            version: "17".parse().unwrap(),
        }
    }
    pub(crate) fn destination(&self) -> PathBuf {
        self.version.definition_base_path(self.home.path())
    }
    pub(crate) fn prepare(&self, revision: &str) -> InstallationStage {
        let stage = InstallationStage::new(&self.destination(), &self.version).unwrap();
        let candidate = stage.candidate();
        fs::create_dir(&candidate).unwrap();
        fs::write(
            candidate.join("metadata.json"),
            r#"{"format_version":1,"server_version_num":170000}"#,
        )
        .unwrap();
        for name in REQUIRED {
            let content = match *name {
                "schema.sql" => revision,
                "pg_type.json"
                | "functions.json"
                | "func_detail.json"
                | "operator_classes.json" => "[]",
                _ => "{}",
            };
            fs::write(candidate.join(name), content).unwrap();
        }
        // The extractor's output lock must be removed with this private stage.
        fs::write(
            candidate.parent().unwrap().join("generation.sql-fun.lock"),
            "",
        )
        .unwrap();
        stage
    }
    pub(crate) fn write_metadata(stage: &InstallationStage, value: &serde_json::Value) {
        fs::write(
            stage.candidate().join("metadata.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    pub(crate) fn assert_revision(&self, revision: &str) {
        validate(&self.destination(), &self.version).unwrap();
        assert_eq!(
            fs::read_to_string(self.destination().join("schema.sql")).unwrap(),
            revision
        );
    }
    pub(crate) fn assert_clean(&self) {
        for entry in fs::read_dir(self.destination().parent().unwrap()).unwrap() {
            assert_eq!(entry.unwrap().file_name(), "17");
        }
    }
    pub(crate) fn occupy(&self, kind: &str) {
        let destination = self.destination();
        match kind {
            "empty" => fs::create_dir(destination).unwrap(),
            "invalid" => {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("metadata.json"), "invalid").unwrap();
            }
            "file" => fs::write(destination, "foreign").unwrap(),
            #[cfg(unix)]
            "symlink" => std::os::unix::fs::symlink(self.home.path(), destination).unwrap(),
            #[cfg(unix)]
            "dangling" => {
                std::os::unix::fs::symlink(self.home.path().join("missing"), destination).unwrap();
            }
            _ => panic!("unknown occupied destination"),
        }
    }
}
