//! server API for `sql-fun`

use clap::Parser;
use std::path::PathBuf;
use typed_openrpc::{OpenRpcInfo, Registry};

#[derive(clap::Parser)]
struct RpcSpecArgs {
    #[clap(
        short,
        long,
        value_name = "OUTPUT_PATH",
        conflicts_with = "output",
        required_unless_present = "output"
    )]
    output_path: Option<PathBuf>,

    #[clap(
        value_name = "OUTPUT_PATH",
        conflicts_with = "output_path",
        required_unless_present = "output_path"
    )]
    output: Option<PathBuf>,

    #[clap(short, long, default_value = "false")]
    force: bool,
}

impl RpcSpecArgs {
    pub fn output_path(&self) -> &PathBuf {
        self.output_path
            .as_ref()
            .unwrap_or_else(|| self.output.as_ref().unwrap())
    }
}

const INFO_DESCRIPTION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/api_description.md"
));

fn main() -> Result<(), anyhow::Error> {
    let args = RpcSpecArgs::parse();

    let mut registry = Registry::new();
    sql_fun_server_api::register_methods(&mut registry);

    let info = OpenRpcInfo {
        title: "sql-fun-server JSON-RPC api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: Some(INFO_DESCRIPTION.to_string()),
    };

    let doc = registry.generate_openrpc_doc_with_info(&info);
    let mut rpc_spec = serde_json::to_string_pretty(&doc)?;
    rpc_spec.push('\n');

    let output_path = args.output_path();
    let path_exists = output_path.exists();
    if path_exists && !args.force {
        anyhow::bail!("output file exists, --force for overwrite")
    }
    std::fs::write(output_path, rpc_spec.as_bytes())?;
    Ok(())
}
