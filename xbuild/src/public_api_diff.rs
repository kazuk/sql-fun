use std::path::{Path, PathBuf};

/// Expose the library's structured diff without parsing terminal output.
#[derive(clap::Args)]
pub(crate) struct PublicApiDiffCommand {
    #[arg(long)]
    old: PathBuf,
    #[arg(long)]
    new: PathBuf,
}

impl PublicApiDiffCommand {
    pub(crate) fn run(&self) -> Result<(), anyhow::Error> {
        let old = read_api(&self.old)?;
        let new = read_api(&self.new)?;
        let diff = public_api::diff::PublicApiDiff::between(old, new);
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for item in diff.added {
            added.push(item.to_string());
        }
        for item in diff.removed {
            removed.push(item.to_string());
        }
        for item in diff.changed {
            changed.push(
                serde_json::json!({"old": item.old.to_string(), "new": item.new.to_string()}),
            );
        }
        serde_json::to_writer(
            std::io::stdout().lock(),
            &serde_json::json!({
                "public_api_version": "0.52.0",
                "added": added, "removed": removed, "changed": changed,
            }),
        )?;
        Ok(())
    }
}

fn read_api(path: &Path) -> Result<public_api::PublicApi, anyhow::Error> {
    let api = public_api::Builder::from_rustdoc_json(path)
        .omit_blanket_impls(false)
        .omit_auto_trait_impls(false)
        .omit_auto_derived_impls(false)
        .include_function_parameter_names(false)
        .build()?;
    Ok(api)
}
