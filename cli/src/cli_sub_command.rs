use crate::{CheckArgs, ExtractArgs};
use sql_fun_core::InitializeArgs;

/// daemon control args
#[derive(clap::Parser, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonControlArgs {
    /// daemon extension name
    daemon_name: String,
    /// daemon arguments
    daemon_args: Vec<String>,
}

/// CLI Subcommand args
#[derive(clap::Subcommand, Debug, Clone, Default)]
pub enum CliSubCommand {
    /// Extract PostgreSQL catalogs or import extension files
    Extract(ExtractArgs),

    /// Get versions
    #[default]
    Versions,

    /// Start daemon extension
    Start(DaemonControlArgs),
    /// Stop daemon extension
    Stop(DaemonControlArgs),
    /// Show status of daemon extension
    Status(DaemonControlArgs),
    /// Restart daemon extension
    Restart(DaemonControlArgs),

    /// External subcommand (captured undefined commands)
    #[clap(external_subcommand)]
    External(Vec<String>),

    /// Initialize `sql-fun` environments
    ///
    #[command(
        name = "init",
        about = "Initialize `sql-fun` environments",
        long_about = r#"Initialize `sql-fun` environments

This command prepares the initial working environment for first-time users.
It asks for missing choices and dispatches to explicitly selected engine setup tasks.
Project metadata is read only when --datasource or a CLI --metadata-file is supplied.
Bare init does not inspect Cargo.toml; ambient metadata settings do not request project access.
PostgreSQL setup installs required builtin metadata from a chosen Docker or existing-server source.
SQLite development-support setup is still under development.
With --yes, supply all required choices through CLI parameters; omitted choices are errors.
Repeated runs preserve valid installations. Incomplete installations must be moved aside before retrying."#,
        after_long_help = r#"Examples:
  Interactive setup (asks for missing choices):
    sql-fun init

  PostgreSQL setup without prompts (all required choices supplied):
    sql-fun init --yes --sql-fun-home /absolute/path/to/sql-fun-home --engines postgres --docker --engine-versions 17,18

  Docker setup requires Docker CLI access to a running daemon and psql.
  --engines and --datasource are alternative selections; do not combine them."#,
        next_line_help = true
    )]
    Initialize(InitializeArgs),

    /// Check application SQL statements
    #[command(
        name = "check",
        about = "Check application embedded SQL statements",
        long_about = r#"Check application embedded SQL statements
This command validates application embedded SQL statement syntax and semantics.
Inference query parameter types, result set column types"#,
        next_line_help = true
    )]
    Check(CheckArgs),
}

impl CliSubCommand {
    /// Construct a builtin extraction command.
    #[must_use]
    pub fn extract(args: &ExtractArgs) -> Self {
        Self::Extract(args.clone())
    }

    pub(crate) const fn is_builtin(&self) -> bool {
        matches!(
            self,
            Self::Initialize(_) | Self::Check(_) | Self::Extract(_)
        )
    }

    pub(crate) async fn run_builtin(
        &self,
        common: &sql_fun_core::SqlFunArgs,
        metadata_file_from_cli: bool,
    ) -> Result<std::process::ExitCode, anyhow::Error> {
        match self {
            Self::Initialize(args) => {
                Ok(crate::init::initialize(common, args, metadata_file_from_cli).await?)
            }
            Self::Check(args) => Ok(args.execute(common).await?),
            Self::Extract(args) => args.execute().await,
            _ => unreachable!("expected builtin command"),
        }
    }

    pub(crate) async fn execute_external(
        &self,
        runnable: &crate::RunnableCommand,
        args: &crate::SqlFunCliArgs,
    ) -> Result<std::process::ExitCode, anyhow::Error> {
        match self {
            Self::External(external_args) => {
                let exit_code = runnable.run_external(args, external_args).await?;
                Ok(std::process::ExitCode::from(
                    u8::try_from(exit_code).unwrap_or(u8::MAX),
                ))
            }
            Self::Start(_daemon_control_args) => todo!(),
            Self::Stop(_daemon_control_args) => todo!(),
            Self::Status(_daemon_control_args) => todo!(),
            Self::Restart(_daemon_control_args) => todo!(),
            Self::Initialize(_initialize_args) => todo!(),
            Self::Check(_check_args) => todo!(),
            Self::Versions => todo!(),
            Self::Extract(_) => unreachable!("builtin extraction"),
        }
    }
    fn daemon_binary_name(name: &str) -> String {
        format!("sqlfun-{name}-daemon")
    }

    fn external_binary_name(name: &str) -> String {
        format!("sqlfun-{name}")
    }

    /// binary name expects
    #[must_use]
    pub fn binary_names(&self) -> Vec<String> {
        let (only_daemon, name) = match self {
            Self::Start(daemon_control_args)
            | Self::Stop(daemon_control_args)
            | Self::Status(daemon_control_args)
            | Self::Restart(daemon_control_args) => (true, daemon_control_args.daemon_name.clone()),
            Self::External(args) => {
                let Some(name) = args.first() else {
                    return Vec::new();
                };
                (false, name.clone())
            }
            Self::Initialize(_) => (false, "init".to_string()),
            Self::Check(_) => (false, "check".to_string()),
            Self::Extract(_) => (false, "extract".to_string()),
            Self::Versions => (false, "versions".to_string()),
        };
        if only_daemon {
            vec![Self::daemon_binary_name(&name)]
        } else {
            vec![
                Self::external_binary_name(&name),
                Self::daemon_binary_name(&name),
            ]
        }
    }
}
