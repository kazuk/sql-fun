use crate::{
    AnalyzeArgs, ApiArgs, CheckArgs, ExtractArgs, RunnableCommand, SqlFunCliArgs, TestArgs,
    init::initialize,
};
use sql_fun_core::InitializeArgs;

/// CLI Subcommand args
#[derive(clap::Subcommand, Debug, Clone)]
pub enum CliSubCommand {
    /// Stream JSON Lines requests to the authenticated per-user server
    Api(ApiArgs),

    /// Analyze SQL without Rust extraction or lockfiles.
    Analyze(AnalyzeArgs),

    /// Test diagnostic sets and result-column contracts without executing SQL.
    Test(TestArgs),

    /// Extract `PostgreSQL` catalogs or import extension files
    Extract(ExtractArgs),

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
    sql-fun init --yes --install-default-theme --sql-fun-home /absolute/path/to/sql-fun-home --engines postgres --docker --engine-versions 17,18

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
    /// Construct a builtin API command.
    #[must_use]
    pub fn api(args: &ApiArgs) -> Self {
        Self::Api(args.clone())
    }

    /// Construct a builtin extraction command.
    #[must_use]
    pub fn extract(args: &ExtractArgs) -> Self {
        Self::Extract(args.clone())
    }

    pub(crate) const fn is_builtin(&self) -> bool {
        matches!(
            self,
            Self::Api(_)
                | Self::Initialize(_)
                | Self::Check(_)
                | Self::Extract(_)
                | Self::Analyze(_)
                | Self::Test(_)
        )
    }

    pub(crate) async fn run_builtin(
        &self,
        common: &sql_fun_core::SqlFunArgs,
        metadata_file_from_cli: bool,
    ) -> Result<std::process::ExitCode, anyhow::Error> {
        match self {
            Self::Api(args) => args.execute().await,
            Self::Analyze(args) => args.execute(common).await,
            Self::Test(args) => args.execute(common).await,
            Self::Initialize(args) => Ok(initialize(common, args, metadata_file_from_cli).await?),
            Self::Check(args) => Ok(args.execute(common).await?),
            Self::Extract(args) => args.execute().await,
            Self::External(_) => unreachable!("expected builtin command"),
        }
    }

    pub(crate) async fn execute_external(
        &self,
        runnable: &RunnableCommand,
        args: &SqlFunCliArgs,
    ) -> Result<std::process::ExitCode, anyhow::Error> {
        match self {
            Self::External(external_args) => {
                let exit_code = runnable.run_external(args, external_args).await?;
                Ok(std::process::ExitCode::from(
                    u8::try_from(exit_code).unwrap_or(u8::MAX),
                ))
            }
            Self::Initialize(_initialize_args) => todo!(),
            Self::Check(_check_args) => todo!(),
            Self::Api(_) => unreachable!("builtin api"),
            Self::Analyze(_) | Self::Test(_) => unreachable!("builtin direct SQL"),
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
        let name = match self {
            Self::External(args) => {
                let Some(name) = args.first() else {
                    return Vec::new();
                };
                name.clone()
            }
            Self::Initialize(_) => "init".to_string(),
            Self::Check(_) => "check".to_string(),
            Self::Analyze(_) => "analyze".to_string(),
            Self::Test(_) => "test".to_string(),
            Self::Extract(_) => "extract".to_string(),
            Self::Api(_) => "api".to_string(),
        };
        vec![
            Self::external_binary_name(&name),
            Self::daemon_binary_name(&name),
        ]
    }
}
