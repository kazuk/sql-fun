use crate::CheckArgs;
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
It creates the sql-fun home directory, downloads or builds the built-in object catalog and extension metadata, and writes configuration files as needed.
Repeated runs are idempotent: existing files are preserved unless you confirm overwrites (or use --yes)."#,
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
