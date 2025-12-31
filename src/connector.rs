use std::{env::var, fs::read_to_string, path::PathBuf};

use toml::Value;

use crate::SqlFunError;

pub enum Connector {
    TokioPostgres,
}

impl Connector {
    pub fn get_connector() -> Result<Connector, SqlFunError> {
        // Read the caller's crate Cargo.toml
        let manifest_dir = var("CARGO_MANIFEST_DIR").map_err(|err| {
            SqlFunError::custom(&format!(
                "failed to get CARGO_MANIFEST_DIR environment valiable : {err}"
            ))
        })?;
        let cargo_toml_path = PathBuf::from(&format!("{manifest_dir}/Cargo.toml"));
        let cargo_toml_content = read_to_string(cargo_toml_path)
            .map_err(|err| SqlFunError::custom(&format!("Failed to read Cargo.toml: {err:?}")))?;
        let cargo_toml: Value = cargo_toml_content
            .parse::<Value>()
            .map_err(|err| SqlFunError::custom(&format!("Failed to parse Cargo.toml: {err:?}")))?;

        // Check the connector value
        let Some(connector) = cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("metadata"))
            .and_then(|meta| meta.get("database"))
            .and_then(|db| db.get("connector"))
            .and_then(|conn| conn.as_str())
        else {
            return Err(SqlFunError::custom(
                "Database connector not specified in Cargo.toml",
            ));
        };

        if connector == "tokio-postgres" {
            return Ok(Connector::TokioPostgres);
        }

        Err(SqlFunError::custom("Unsupported database connector"))
    }
}
