use std::{
    ffi::OsStr,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    process::{ExitCode, Stdio},
};

use cargo_metadata::MetadataCommand;
use sql_fun_core::SqlFunArgs;
use syn::File;

/// Checks application embedded SQL statements
#[derive(clap::Parser, Debug, Clone)]
#[expect(clippy::struct_excessive_bools)]
pub struct CheckArgs {
    /// Check all packages in the cargo workspace
    #[clap(long, help_heading = Self::PACKAGE_SELECTION_HEADING, conflicts_with ="package")]
    workspace: bool,

    /// package to check
    #[clap(short, long, help_heading = Self::PACKAGE_SELECTION_HEADING, conflicts_with = "workspace")]
    package: Option<String>,

    /// check only this package's library
    #[clap(long, help_heading = Self::TARGET_SELECTION_HEADING)]
    lib: bool,

    /// check only the specified binary
    #[clap(long, help_heading = Self::TARGET_SELECTION_HEADING)]
    bin: Option<String>,

    /// check only the specified example
    #[clap(long, help_heading = Self::TARGET_SELECTION_HEADING)]
    example: Option<String>,

    /// check only the specified test
    #[clap(long, help_heading = Self::TARGET_SELECTION_HEADING)]
    test: Option<String>,

    /// include tests when check the lib or bin
    #[clap(long, help_heading = Self::TARGET_SELECTION_HEADING)]
    tests: bool,

    /// check only the specified bench
    #[clap(long, help_heading =  Self::TARGET_SELECTION_HEADING)]
    bench: Option<String>,

    /// space or comma separated list of features
    #[clap(short('F'), long, help_heading = Self::FEATURE_SELECTION_HEADING, conflicts_with ="all_features")]
    features: Vec<String>,

    /// Activate all available feature
    #[clap(long, help_heading = Self::FEATURE_SELECTION_HEADING)]
    all_features: bool,

    /// Do not activate the `default` feature
    #[clap(long, help_heading = Self::FEATURE_SELECTION_HEADING, conflicts_with ="all_features")]
    no_default_features: bool,

    /// Force rebuild schema cache
    ///
    /// When specified, Clean schema dump cache before checking, force parse schema dump and rebuild schema information
    #[clap(long, help_heading = Self::SCHEMA_CACHE_CONTROL_HEADING, conflicts_with = "validate_only")]
    force_rebuild: bool,

    /// Validate only
    ///
    /// When specified, Fail if schema dump or SQL statement change.
    #[clap(long, help_heading = Self::SCHEMA_CACHE_CONTROL_HEADING, conflicts_with = "force_rebuild")]
    validate_only: bool,

    /// Expect Schema dump not changed
    ///
    /// When specified, Fail if schema dump changed
    #[clap(long, help_heading = Self::SCHEMA_CACHE_CONTROL_HEADING, conflicts_with = "force_rebuild")]
    expect_schema_dump_valid: bool,

    /// Number of parallel jobs, default to # of CPUs
    #[clap(short('j'), long, help_heading = Self::COMPILATION_OPTIONS_HEADERS)]
    jobs: Option<usize>,

    /// Build artifacts in release mode, with optimizations
    #[clap(long, help_heading = Self::COMPILATION_OPTIONS_HEADERS)]
    release: bool,

    /// Build artifacts with the specified profile
    #[clap(long, help_heading = Self::COMPILATION_OPTIONS_HEADERS)]
    profile: Option<String>,

    /// Target triple which compiles will be for
    #[clap(long, help_heading = Self::COMPILATION_OPTIONS_HEADERS)]
    target: Option<String>,

    /// Directory for all generated artifacts
    #[clap(long, help_heading = Self::COMPILATION_OPTIONS_HEADERS)]
    target_dir: Option<PathBuf>,

    /// Path to Cargo.toml
    #[clap(long, help_heading = Self::MANIFEST_OPTIONS_HEADERS)]
    manifest_path: Option<PathBuf>,

    /// Require Cargo.lock and cache are up to date
    #[clap(long, help_heading = Self::MANIFEST_OPTIONS_HEADERS)]
    frozen: bool,

    /// Require Cargo.lock is up to date
    #[clap(long, help_heading = Self::MANIFEST_OPTIONS_HEADERS)]
    locked: bool,

    /// Run without accessing the network
    #[clap(long, help_heading = Self::MANIFEST_OPTIONS_HEADERS)]
    offline: bool,
}

impl CheckArgs {
    const PACKAGE_SELECTION_HEADING: &str = "Package Selection";
    const TARGET_SELECTION_HEADING: &str = "Target Selection";
    const FEATURE_SELECTION_HEADING: &str = "Feature Selection";
    const SCHEMA_CACHE_CONTROL_HEADING: &str = "Schema Cache Control";
    const COMPILATION_OPTIONS_HEADERS: &str = "Compilation Options";
    const MANIFEST_OPTIONS_HEADERS: &str = "Manifest Options";

    /// execute check cli command
    ///
    /// # Errors
    ///
    /// [`anyhow::Error`] for runtime error
    ///
    pub async fn execute(&self, _common_args: &SqlFunArgs) -> Result<ExitCode, anyhow::Error> {
        if self.workspace {
            let deps_members = Self::dependent_members()?;
            for member in deps_members {
                self.expand_member(member.as_str()).await?;
            }
        } else if let Some(member) = &self.package {
            self.expand_member(member).await?;
        } else {
            let default_members = Self::default_packages()?;
            for member in default_members {
                self.expand_member(&member).await?;
            }
        }

        todo!()
    }

    async fn expand_member(&self, member: &str) -> Result<(), anyhow::Error> {
        let mut cargo_expand_command = CargoExpand::new(member)?;
        cargo_expand_command.set("--lib", self.lib);
        cargo_expand_command.set("--tests", self.tests);
        cargo_expand_command.set_value("--bin", self.bin.as_deref());
        cargo_expand_command.set_value("--example", self.example.as_deref());
        cargo_expand_command.set_value("--test", self.test.as_deref());
        cargo_expand_command.set_value("--bench", self.bench.as_deref());
        cargo_expand_command.set("--all-features", self.all_features);
        cargo_expand_command.set("--no-default-features", self.no_default_features);
        cargo_expand_command.set_values("--features", &self.features);
        cargo_expand_command.set_value_string("--jobs", self.jobs.map(|jobs| format!("{jobs}")));
        cargo_expand_command.set("--release", self.release);
        cargo_expand_command.set_value("--profile", self.profile.as_deref());
        cargo_expand_command.set_value("--target", self.target.as_deref());
        cargo_expand_command.set_value(
            "--target-dir",
            self.target_dir
                .as_ref()
                .map(|v| {
                    v.as_os_str()
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("can not convert OsStr::to_str"))
                })
                .transpose()?,
        );
        cargo_expand_command.set_value(
            "--manifest-path",
            self.manifest_path
                .as_ref()
                .map(|v| {
                    v.as_os_str()
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("can not convert OsStr::to_str"))
                })
                .transpose()?,
        );
        let file = cargo_expand_command.exec().await?;
        let requirements = sql_fun_codegen::QueryRequirements::extract(&file).map_err(|error| {
            anyhow::anyhow!("failed to read query requirements for {member}: {error}")
        })?;
        log::debug!(
            "extracted {} query requirements from {member}",
            requirements.len()
        );
        Ok(())
    }

    fn dependent_members() -> Result<Vec<String>, anyhow::Error> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let mut deps_members = Vec::new();
        for member in metadata.workspace_members {
            if let Some(member_package) = metadata.packages.iter().find(|p| p.id == member)
                && Self::depends_sql_fun(member_package)
            {
                deps_members.push(member.repr);
            }
        }
        Ok(deps_members)
    }

    fn default_packages() -> Result<Vec<String>, anyhow::Error> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let mut deps_members = Vec::new();
        for package in metadata.workspace_default_packages() {
            if Self::depends_sql_fun(package) {
                deps_members.push(package.id.repr.clone());
            }
        }
        Ok(deps_members)
    }

    fn depends_sql_fun(package: &cargo_metadata::Package) -> bool {
        package
            .dependencies
            .iter()
            .find(|d| d.name == "sql-fun")
            .is_some()
    }
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "Keep tests next to their implementation."
)]
mod test_check_args {
    use super::CheckArgs;
    use ::clap::Parser;

    #[test]
    fn offline_is_a_flag_and_check_arguments_are_valid() {
        let args = CheckArgs::try_parse_from(["check", "--offline"]).unwrap();
        assert!(args.offline);
    }
}

pub struct CargoExpand(tokio::process::Command, std::fs::File);

impl CargoExpand {
    pub fn new(package: &str) -> Result<Self, anyhow::Error> {
        let temp = tempfile::tempfile()?;

        let mut command = tokio::process::Command::new("cargo");
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(temp.try_clone()?));
        command.arg("expand");
        command.env("SQL_FUN_MODE", "CHECK");
        command.args(["--package", package]);

        Ok(Self(command, temp))
    }

    pub fn set(&mut self, option_name: &str, value: bool) -> &mut Self {
        if value {
            self.0.arg(option_name);
        }
        self
    }

    pub fn set_value(&mut self, option_name: &str, value: Option<&str>) -> &mut Self {
        if let Some(value) = value {
            self.0.args([option_name, value]);
        }
        self
    }

    pub fn set_value_string(&mut self, option_name: &str, value: Option<String>) -> &mut Self {
        if let Some(value) = value {
            self.0.args([option_name, value.as_str()]);
        }
        self
    }

    pub fn set_values<'a>(
        &'a mut self,
        option_name: &str,
        values: &[impl AsRef<OsStr>],
    ) -> &'a mut Self {
        if !values.is_empty() {
            self.0.arg(option_name);
            self.0.args(values);
        }
        self
    }

    pub async fn exec(mut self) -> Result<syn::File, anyhow::Error> {
        let status = self.0.status().await?;

        if status.success() {
            self.1.seek(SeekFrom::Start(0))?;
            let mut rust_source = String::new();
            self.1.read_to_string(&mut rust_source)?;
            let rust_source_file: File = ::syn::parse_file(&rust_source)?;
            Ok(rust_source_file)
        } else {
            anyhow::bail!("cargo-expand exited non-success ({status})");
        }
    }
}
