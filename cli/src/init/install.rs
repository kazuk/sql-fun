use std::{
    fs::{self, File},
    path::Path,
};

use ::anyhow::Context;
use ::sql_fun_core::{InitializeArgs, PostgresVersion};

const REQUIRED: &[&str] = &[
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

pub(super) fn validate(path: &Path, version: &PostgresVersion) -> Result<(), anyhow::Error> {
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
    // Optional on legacy format-1 installations; current extraction always emits it.
    let memberships = path.join("btree_operator_memberships.json");
    let membership_info = match fs::symlink_metadata(&memberships) {
        Ok(info) => Some(info),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(info) = membership_info {
        anyhow::ensure!(
            info.is_file() && !info.file_type().is_symlink(),
            "invalid B-tree membership file"
        );
        let payload: serde_json::Value = serde_json::from_slice(&fs::read(memberships)?)?;
        anyhow::ensure!(
            payload["format_version"] == 1 && payload["memberships"].is_array(),
            "unsupported B-tree membership payload"
        );
    }
    Ok(())
}

pub(super) async fn install(
    args: &InitializeArgs,
    home: &Path,
    version: &PostgresVersion,
) -> Result<(), anyhow::Error> {
    let destination = version.definition_base_path(home);
    let parent = destination
        .parent()
        .context("builtin installation parent")?;
    fs::create_dir_all(parent)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(format!(".{version}.init.lock")))?;
    lock.try_lock()
        .context("another initialization owns this PostgreSQL version")?;
    if destination.exists() {
        validate(&destination, version).context("existing builtin installation is incomplete or incompatible; move it aside before retrying")?;
        println!(
            "PostgreSQL {version}: already initialized at {}",
            destination.display()
        );
        return Ok(());
    }
    let stage = tempfile::Builder::new()
        .prefix(".init-")
        .tempdir_in(parent)?;
    let candidate = stage.path().join("generation");
    crate::extract::initialize_catalog(args, version, &candidate).await?;
    validate(&candidate, version)?;
    // The per-version lock serializes cooperating installers.
    anyhow::ensure!(
        !destination.exists(),
        "builtin destination appeared during initialization"
    );
    fs::rename(&candidate, &destination)?;
    File::open(parent)?.sync_all()?;
    println!(
        "PostgreSQL {version}: initialized at {}",
        destination.display()
    );
    Ok(())
}
