use crate::{SqlFunArgsBuilder, SqlFunArgsBuilderError};
use std::{collections::HashMap, path::PathBuf};

#[derive(
    clap::Args, Debug, serde::Serialize, serde::Deserialize, Clone, derive_builder::Builder,
)]
#[builder(build_fn(error = "SqlFunArgsBuilderError"))]
pub struct LoggingArgs {
    /// Log level (trace, debug, info, warn, error)
    #[clap(long, env = Self::SQL_FUN_LOG_LEVEL, default_value = "info")]
    #[builder(
        setter(into, strip_option),
        default = "String::from(Self::SQL_FUN_LOG_LEVEL_DEFAULT)"
    )]
    log_level: String,

    /// Path to log file (stderr if none)
    #[clap(long, env = Self::SQL_FUN_LOG_FILE)]
    #[builder(setter(into, strip_option), default)]
    log_file: Option<PathBuf>,
}

impl LoggingArgsBuilder {
    const SQL_FUN_LOG_LEVEL_DEFAULT: &str = "info";
}

impl SqlFunArgsBuilder {
    /// Log level (trace, debug, info, warn, error)
    pub fn log_level<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<String>,
    {
        self.log_args.log_level(value);
        self
    }

    /// Path to log file (stderr if none)
    pub fn log_file<Value>(&mut self, value: Value) -> &mut Self
    where
        Value: Into<PathBuf>,
    {
        self.log_args.log_file(value);
        self
    }
}

impl LoggingArgs {
    const SQL_FUN_LOG_LEVEL: &str = "SQL_FUN_LOG_LEVEL";
    const SQL_FUN_LOG_FILE: &str = "SQL_FUN_LOG_FILE";

    pub(crate) fn apply_envs(builder: &mut SqlFunArgsBuilder, envs: &HashMap<String, String>) {
        if let Some(value) = envs.get(Self::SQL_FUN_LOG_LEVEL) {
            builder.log_level(value);
        }
        if let Some(value) = envs.get(Self::SQL_FUN_LOG_FILE) {
            builder.log_file(PathBuf::from(value));
        }
    }

    pub fn get_envs(&self, envs: &mut HashMap<String, String>) {
        if let Some(log_file) = &self.log_file {
            envs.insert(
                Self::SQL_FUN_LOG_FILE.to_string(),
                log_file.to_string_lossy().to_string(),
            );
        }
        envs.insert(Self::SQL_FUN_LOG_LEVEL.to_string(), self.log_level.clone());
    }

    pub fn set_log_level(&mut self, log_level: &str) {
        self.log_level = log_level.to_string();
    }
}
