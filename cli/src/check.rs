mod diagnostics;
mod lockfiles;
mod progress;
mod submission;

use self::progress::{Progress, ProgressMode};

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
    /// Progress output on stdout (CLI overrides `SQL_FUN_CHECK_PROGRESS`).
    #[clap(
        long,
        env = "SQL_FUN_CHECK_PROGRESS",
        value_enum,
        default_value = "names"
    )]
    progress: ProgressMode,

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

    /// Build the lockfile from an empty initial state
    #[clap(long, help_heading = Self::LOCKFILE_CONTROL_HEADING, conflicts_with = "validate_only")]
    force_rebuild: bool,

    /// Validate only
    ///
    /// When specified, Fail if schema dump or SQL statement change.
    #[clap(long, help_heading = Self::LOCKFILE_CONTROL_HEADING, conflicts_with = "force_rebuild")]
    validate_only: bool,

    /// Expect Schema dump not changed
    ///
    /// When specified, Fail if schema dump changed
    #[clap(long, help_heading = Self::LOCKFILE_CONTROL_HEADING, conflicts_with = "force_rebuild")]
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
    const LOCKFILE_CONTROL_HEADING: &str = "Lockfile Control";
    const COMPILATION_OPTIONS_HEADERS: &str = "Compilation Options";
    const MANIFEST_OPTIONS_HEADERS: &str = "Manifest Options";

    /// execute check cli command
    ///
    /// # Errors
    ///
    /// [`anyhow::Error`] for runtime error
    ///
    pub async fn execute(&self, common_args: &SqlFunArgs) -> Result<ExitCode, anyhow::Error> {
        ::which::which("cargo-expand").map_err(|_| anyhow::anyhow!(
            "sql-fun check requires cargo-expand on PATH; install it with \"cargo install --locked cargo-expand\" and ensure Cargo's binary directory is on PATH"
        ))?;
        let mut progress = Progress::new(std::io::stdout(), self.progress);
        let result = self.execute_with_progress(common_args, &mut progress).await;
        // Finish framing even when extraction, RPC, or finalization fails.
        progress.finish(matches!(&result, Ok(code) if *code == ExitCode::SUCCESS))?;
        result
    }

    async fn execute_with_progress<W: std::io::Write>(
        &self,
        common_args: &SqlFunArgs,
        progress: &mut Progress<W>,
    ) -> Result<ExitCode, anyhow::Error> {
        progress.phase("Extracting SQL queries from selected Cargo targets")?;
        let mut requirements = Vec::new();
        let members = if self.workspace {
            Self::dependent_members()?
        } else if let Some(member) = &self.package {
            vec![member.clone()]
        } else {
            Self::default_packages()?
        };
        for member in members {
            requirements.extend(self.expand_member(&member, common_args).await?);
        }
        if requirements.is_empty() {
            eprintln!("warning: no SQL queries found in the selected targets");
            return Ok(ExitCode::SUCCESS);
        }
        progress.set_total(requirements.len());
        let lockfiles = lockfiles::Lockfiles::new(
            common_args,
            &requirements,
            self.force_rebuild,
            self.expect_schema_dump_valid,
        )?;
        let client = crate::api::ApiArgs::from_environment()?.connect().await?;
        self.process_analyses(client, common_args, requirements, lockfiles, progress)
            .await
    }

    async fn process_analyses<W: std::io::Write>(
        &self,
        client: ::sql_fun_client::TcpJsonRpcClientAsync,
        args: &SqlFunArgs,
        requirements: Vec<::sql_fun_codegen::QueryRequirement>,
        lockfiles: lockfiles::Lockfiles,
        progress: &mut Progress<W>,
    ) -> Result<ExitCode, anyhow::Error> {
        self.process_analyses_to(
            client,
            args,
            requirements,
            lockfiles,
            progress,
            &mut std::io::stderr(),
        )
        .await
    }

    async fn process_analyses_to<W: std::io::Write, D: std::io::Write>(
        &self,
        mut client: ::sql_fun_client::TcpJsonRpcClientAsync,
        args: &SqlFunArgs,
        requirements: Vec<::sql_fun_codegen::QueryRequirement>,
        mut lockfiles: lockfiles::Lockfiles,
        progress: &mut Progress<W>,
        diagnostics_output: &mut D,
    ) -> Result<ExitCode, anyhow::Error> {
        progress.begin(requirements.len())?;
        let renderer = crate::diagnostic::DiagnosticRenderer::from_args(args)?;
        let (sender, mut receiver) = ::tokio::sync::mpsc::channel(8);
        let producer = async {
            let submitted = submission::submit(&mut client, args, requirements, sender).await;
            let finished = client.finish().await;
            submitted?;
            finished?;
            Ok::<(), anyhow::Error>(())
        };
        let consumer = async {
            let mut failure = None;
            while let Some(analysis) = receiver.recv().await {
                analysis.log_received();
                let reported = progress.received(analysis.function_name());
                if let Err(error) = reported
                    && failure.is_none()
                {
                    failure = Some(anyhow::Error::from(error));
                }
                if let Err(error) =
                    diagnostics::render_all(&analysis, &renderer, diagnostics_output)
                    && failure.is_none()
                {
                    failure = Some(error);
                }
                if let Some(error) = analysis.tokenization_error()
                    && failure.is_none()
                {
                    failure = Some(anyhow::anyhow!(
                        "cannot tokenize SQL diagnostic for {}: {error}",
                        analysis.function_name()
                    ));
                }
                if failure.is_none()
                    && let Err(error) = lockfiles.receive(&analysis)
                {
                    failure = Some(error);
                }
            }
            if let Some(error) = failure {
                Err(error)
            } else {
                Ok(())
            }
        };
        let (completed, processed) = ::tokio::join!(producer, consumer);
        completed?;
        processed?;
        let phase = if self.validate_only {
            "Validating lockfiles"
        } else {
            "Writing lockfiles"
        };
        progress.phase(phase)?;
        if lockfiles.finalize(self.validate_only)? {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::FAILURE)
        }
    }

    async fn expand_member(
        &self,
        member: &str,
        common_args: &SqlFunArgs,
    ) -> Result<::sql_fun_codegen::QueryRequirements, anyhow::Error> {
        let mut cargo_expand_command = CargoExpand::new(member, common_args)?;
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
        Ok(requirements)
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
        package.dependencies.iter().any(|d| d.name == "sql-fun")
    }
}

#[cfg(test)]
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
    pub fn new(package: &str, common_args: &SqlFunArgs) -> Result<Self, anyhow::Error> {
        let temp = tempfile::tempfile()?;

        let mut command = tokio::process::Command::new("cargo");
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(temp.try_clone()?));
        command.arg("expand");
        command.envs(common_args.environments()?);
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

#[cfg(test)]
mod test_cargo_expand {
    use super::CargoExpand;
    use ::sql_fun_core::SqlFunArgs;

    #[rstest::rstest]
    #[case::query_selected(None)]
    #[case::cli_selected(Some("selected"))]
    fn forwards_explicit_configuration_in_check_mode(#[case] datasource: Option<&str>) {
        let project = ::tempfile::tempdir().unwrap();
        let metadata = project.path().join("selected.toml");
        std::fs::write(
            &metadata,
            r#"
[package.metadata.database.selected]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
search_path = ["chosen"]
"#,
        )
        .unwrap();
        std::fs::write(
            project.path().join("selected.sql"),
            "CREATE TABLE entries(id int4);",
        )
        .unwrap();
        let catalog = project.path().join("selected-catalog");
        let args = SqlFunArgs::builder(&std::collections::HashMap::new())
            .unwrap()
            .sql_fun_home(project.path())
            .metadata_file(metadata.to_str().unwrap())
            .schema_file(project.path().join("selected.sql"))
            .cte_catalog_dir(&catalog)
            .datasource(datasource.map(str::to_owned))
            .build()
            .unwrap();
        let expansion = CargoExpand::new("selected-package", &args).unwrap();
        let environments: std::collections::HashMap<_, _> = expansion
            .0
            .as_std()
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_str().unwrap().to_owned(),
                    value.unwrap().to_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(environments["SQL_FUN_MODE"], "CHECK");
        assert_eq!(
            environments["SQL_FUN_METADATA_FILE"],
            metadata.to_str().unwrap()
        );
        assert_eq!(
            environments["SQL_FUN_CTE_CATALOG_DIR"],
            catalog.to_str().unwrap()
        );
        let restored = SqlFunArgs::from_environments(&environments).unwrap();
        assert_eq!(restored.datasource(), datasource);
        assert_eq!(restored.metadata_file().unwrap(), metadata);
    }
}
