//! Minimal real-server lifecycle CLI.

use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::Parser)]
#[command(about = "Connect to an authenticated per-user sql-fun server, then disconnect")]
struct Args {
    /// Absolute path of the expected server executable.
    #[arg(long)]
    server_executable: PathBuf,
    /// Explicit user namespace under /tmp.
    #[arg(long)]
    user: String,
    /// Optional startup command, executed by /bin/sh -c in the current directory.
    #[arg(long)]
    server_command: Option<String>,
    /// Total connection timeout in milliseconds (default: 40000).
    #[arg(long)]
    timeout_ms: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = <Args as ::clap::Parser>::parse();
    let options = ::sql_fun_client::ConnectionOptions::builder()
        .server_executable(&args.server_executable)
        .user(&args.user)
        .maybe_server_command(args.server_command.as_deref())
        .maybe_timeout(args.timeout_ms.map(Duration::from_millis))
        .build();
    let client = ::sql_fun_client::connect(&options).await?;
    println!("Authenticated server at {}", client.peer_addr()?);
    drop(client);
    Ok(())
}
