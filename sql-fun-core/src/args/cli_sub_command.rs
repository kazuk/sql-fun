use crate::{DaemonControlArgs, InitializeArgs};

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
}

impl CliSubCommand {
    fn daemon_binary_name(name: &str) -> String {
        format!("sqlfun-{name}-daemon")
    }

    fn external_binary_name(name: &str) -> String {
        format!("sqlfun-{name}")
    }

    /// binary name expects
    pub fn binary_names(&self) -> Vec<String> {
        let (only_deamon, name) = match self {
            CliSubCommand::Start(daemon_control_args) => (true, &daemon_control_args.daemon_name),
            CliSubCommand::Stop(daemon_control_args) => (true, &daemon_control_args.daemon_name),
            CliSubCommand::Status(daemon_control_args) => (true, &daemon_control_args.daemon_name),
            CliSubCommand::Restart(daemon_control_args) => (true, &daemon_control_args.daemon_name),
            CliSubCommand::External(args) => {
                assert!(!args.is_empty()); // clap guarantees args non empty
                (false, &args[0])
            }
            CliSubCommand::Initialize(_) => unreachable!(),
            CliSubCommand::Versions => todo!(),
        };
        if only_deamon {
            vec![Self::daemon_binary_name(name)]
        } else {
            vec![
                Self::external_binary_name(name),
                Self::daemon_binary_name(name),
            ]
        }
    }
}
