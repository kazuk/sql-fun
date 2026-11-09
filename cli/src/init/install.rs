use crate::extract::initialize_catalog;
use std::{
    fs::{self, File},
    io::ErrorKind,
    path::{Path, PathBuf},
};

use ::anyhow::Context;
use ::sql_fun_core::{InitializeArgs, PostgresVersion};

pub(crate) const REQUIRED: &[&str] = &[
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
];

pub(crate) fn validate(path: &Path, version: &PostgresVersion) -> Result<(), anyhow::Error> {
    let info = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        info.is_dir() && !info.file_type().is_symlink(),
        "builtin installation must be a real directory"
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("metadata.json"))?)?;
    anyhow::ensure!(
        metadata["format_version"] == 1,
        "unsupported builtin metadata format"
    );
    anyhow::ensure!(
        metadata["server_version_num"]
            .as_u64()
            .map(|number| number / 10_000)
            == Some(u64::from(version.major())),
        "builtin metadata major does not match {version}"
    );
    // Older stable format-1 artifacts omitted version_identity.
    if let Some(identity) = metadata["version_identity"].as_str() {
        anyhow::ensure!(
            identity == version.to_string(),
            "builtin metadata identity does not match {version}"
        );
    } else {
        anyhow::ensure!(
            version.is_stable(),
            "prerelease metadata requires an explicit identity"
        );
    }
    for name in REQUIRED {
        let file = path.join(name);
        let info =
            fs::symlink_metadata(&file).with_context(|| format!("missing builtin file {name}"))?;
        anyhow::ensure!(
            info.is_file() && !info.file_type().is_symlink(),
            "invalid builtin file {name}"
        );
        let content = fs::read(&file)?;
        anyhow::ensure!(!content.is_empty(), "empty builtin file {name}");
        if *name != "schema.sql" {
            let catalog: serde_json::Value = serde_json::from_slice(&content)
                .with_context(|| format!("invalid builtin JSON {name}"))?;
            let valid_shape = if [
                "pg_type.json",
                "functions.json",
                "func_detail.json",
                "operator_classes.json",
            ]
            .contains(name)
            {
                catalog.is_array()
            } else {
                catalog.is_object()
            };
            anyhow::ensure!(valid_shape, "invalid builtin catalog structure in {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_validate {
    use crate::init::install::validate;
    use crate::test_helpers::installation::InstallationFixture;

    #[rstest::rstest]
    #[case::absent(None)]
    #[case::older(Some("0.1.0"))]
    #[case::newer(Some("99.0.0"))]
    #[case::opaque(Some("legacy-extractor"))]
    fn extractor_version_does_not_control_compatibility(#[case] extractor: Option<&str>) {
        let fixture = InstallationFixture::new();
        let stage = fixture.prepare("compatible");
        let mut metadata = serde_json::json!({"format_version": 1, "server_version_num": 170_000});
        if let Some(version) = extractor {
            metadata["extractor_version"] = version.into();
        }
        InstallationFixture::write_metadata(&stage, &metadata);
        validate(&stage.candidate(), &"17".parse().unwrap()).unwrap();
    }

    #[rstest::rstest]
    #[case::format("format_version", serde_json::json!(2))]
    #[case::major("server_version_num", serde_json::json!(180_000))]
    #[case::identity("version_identity", serde_json::json!("18"))]
    fn extractor_version_does_not_bypass_compatibility(
        #[case] key: &str,
        #[case] value: serde_json::Value,
    ) {
        let fixture = InstallationFixture::new();
        let stage = fixture.prepare("incompatible");
        let mut metadata = serde_json::json!({
            "format_version": 1, "server_version_num": 170_000, "extractor_version": "0.1.0"
        });
        metadata[key] = value;
        InstallationFixture::write_metadata(&stage, &metadata);
        assert!(validate(&stage.candidate(), &"17".parse().unwrap()).is_err());
    }
}

fn installed(destination: &Path, version: &PostgresVersion) -> Result<bool, anyhow::Error> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            validate(destination, version).context("existing builtin installation is incomplete or incompatible; move it aside before retrying")?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Owns only this initializer's private extraction output and its cleanup.
pub(crate) struct InstallationStage {
    stage: tempfile::TempDir,
    destination: PathBuf,
    version: PostgresVersion,
}

impl InstallationStage {
    pub(crate) fn new(
        destination: &Path,
        version: &PostgresVersion,
    ) -> Result<Self, anyhow::Error> {
        let parent = destination
            .parent()
            .context("builtin installation parent")?;
        fs::create_dir_all(parent)?;
        let stage = tempfile::Builder::new()
            .prefix(&format!("{version}.installing."))
            .tempdir_in(parent)?;
        Ok(Self {
            stage,
            destination: destination.to_owned(),
            version: *version,
        })
    }

    pub(crate) fn candidate(&self) -> PathBuf {
        self.stage.path().join("generation")
    }

    /// Returns whether this candidate won publication. Race losers validate the winner.
    fn publish(self) -> Result<bool, anyhow::Error> {
        validate(&self.candidate(), &self.version)?;
        if installed(&self.destination, &self.version)? {
            return Ok(false);
        }
        self.publish_validated()
    }

    fn publish_validated(self) -> Result<bool, anyhow::Error> {
        match fs::rename(self.candidate(), &self.destination) {
            Ok(()) => {
                File::open(
                    self.destination
                        .parent()
                        .context("builtin installation parent")?,
                )?
                .sync_all()?;
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty
                ) =>
            {
                if installed(&self.destination, &self.version)? {
                    Ok(false)
                } else {
                    Err(error.into())
                }
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod test_publication {
    use crate::init::install::InstallationStage;

    use crate::{
        init::install::{installed, validate},
        test_helpers::installation::InstallationFixture,
    };
    use std::{fs, sync::Barrier, thread};

    #[test]
    fn concurrent_initializers_preserve_one_complete_generation() {
        let fixture = InstallationFixture::new();
        let first = fixture.prepare("first");
        let second = fixture.prepare("second");
        let ready = Barrier::new(2);
        let outcomes = thread::scope(|scope| {
            let publish = |stage: InstallationStage| {
                validate(&stage.candidate(), &stage.version).unwrap();
                assert!(!installed(&stage.destination, &stage.version).unwrap());
                ready.wait();
                stage.publish_validated().unwrap()
            };
            let first = scope.spawn(move || publish(first));
            let second = scope.spawn(move || publish(second));
            while !first.is_finished() || !second.is_finished() {
                // Publication must never expose an incomplete generation to readers.
                installed(&fixture.destination(), &"17".parse().unwrap()).unwrap();
                thread::yield_now();
            }
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(outcomes.0, outcomes.1);
        fixture.assert_revision(if outcomes.0 { "first" } else { "second" });
        fixture.assert_clean();
    }

    #[test]
    fn destination_appearing_during_preparation_is_preserved() {
        let fixture = InstallationFixture::new();
        let loser = fixture.prepare("loser");
        assert!(fixture.prepare("winner").publish().unwrap());
        assert!(!loser.publish().unwrap());
        fixture.assert_revision("winner");
        fixture.assert_clean();
    }

    #[test]
    fn incomplete_candidate_never_becomes_visible() {
        let fixture = InstallationFixture::new();
        let stage = fixture.prepare("incomplete");
        fs::remove_file(stage.candidate().join("casts.json")).unwrap();
        assert!(stage.publish().is_err());
        assert!(!fixture.destination().exists());
        fixture.assert_clean();
    }

    #[test]
    fn extraction_failure_cleans_only_its_own_stage() {
        let fixture = InstallationFixture::new();
        let other = fixture.prepare("other");
        let failed = fixture.prepare("incomplete");
        drop(failed);
        assert!(other.candidate().exists());
        assert!(!fixture.destination().exists());
        drop(other);
        fixture.assert_clean();
    }

    #[rstest::rstest]
    #[case::empty("empty")]
    #[case::invalid("invalid")]
    #[case::file("file")]
    #[cfg_attr(unix, case::symlink("symlink"))]
    #[cfg_attr(unix, case::dangling("dangling"))]
    fn invalid_destination_is_rejected_and_preserved(#[case] kind: &str) {
        let fixture = InstallationFixture::new();
        let stage = fixture.prepare("candidate");
        fixture.occupy(kind);
        let before = fs::symlink_metadata(fixture.destination())
            .unwrap()
            .file_type();
        assert!(installed(&fixture.destination(), &"17".parse().unwrap()).is_err());
        assert!(stage.publish().is_err());
        assert_eq!(
            fs::symlink_metadata(fixture.destination())
                .unwrap()
                .file_type(),
            before
        );
        fixture.assert_clean();
    }

    #[test]
    fn invalid_winner_at_rename_remains_an_error() {
        let fixture = InstallationFixture::new();
        let stage = fixture.prepare("candidate");
        validate(&stage.candidate(), &stage.version).unwrap();
        assert!(!installed(&stage.destination, &stage.version).unwrap());
        fixture.occupy("invalid");
        let error = stage.publish_validated().unwrap_err();
        assert!(error.to_string().contains("move it aside"));
        assert_eq!(
            fs::read_to_string(fixture.destination().join("metadata.json")).unwrap(),
            "invalid"
        );
        fixture.assert_clean();
    }

    #[test]
    fn unrelated_rename_error_is_not_reported_as_success() {
        let fixture = InstallationFixture::new();
        let mut stage = fixture.prepare("candidate");
        // A destination below the candidate cannot be the target of its rename.
        stage.destination = stage.candidate().join("nested");
        let error = stage.publish().unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert!(!fixture.destination().exists());
        fixture.assert_clean();
    }
}

pub(super) async fn install(
    args: &InitializeArgs,
    home: &Path,
    version: &PostgresVersion,
) -> Result<(), anyhow::Error> {
    let destination = version.definition_base_path(home);
    let published = if installed(&destination, version)? {
        false
    } else {
        let stage = InstallationStage::new(&destination, version)?;
        initialize_catalog(args, version, &stage.candidate()).await?;
        stage.publish()?
    };
    let status = if published {
        "initialized"
    } else {
        "already initialized"
    };
    println!(
        "PostgreSQL {version}: {status} at {}",
        destination.display()
    );
    Ok(())
}
