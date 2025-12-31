use std::{collections::HashMap, path::PathBuf};

use crate::SqlFunArgsError;

#[derive(clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct LoggingArgs {
    /// Log level (trace, debug, info, warn, error)
    #[clap(long, env = Self::SQL_FUN_LOG_LEVEL, default_value = "info")]
    log_level: String,

    /// Path to log file (stderr if none)
    #[clap(long, env = Self::SQL_FUN_LOG_FILE)]
    log_file: Option<PathBuf>,
}

impl LoggingArgs {
    const SQL_FUN_LOG_LEVEL: &str = "SQL_FUN_LOG_LEVEL";
    const SQL_FUN_LOG_FILE: &str = "SQL_FUN_LOG_FILE";

    pub fn get_envs(&self, envs: &mut HashMap<String, String>) -> Result<(), SqlFunArgsError> {
        if let Some(log_file) = &self.log_file {
            insert_to_result(envs, Self::SQL_FUN_LOG_FILE, &log_file.to_string_lossy());
        }
        insert_to_result(envs, Self::SQL_FUN_LOG_LEVEL, &self.log_level);

        return Ok(());

        fn insert_to_result(result: &mut HashMap<String, String>, name: &str, value: &str) {
            result.insert(name.to_string(), value.to_string());
        }
    }

    pub fn set_log_level(&mut self, log_level: &str) {
        self.log_level = log_level.to_string()
    }
}
