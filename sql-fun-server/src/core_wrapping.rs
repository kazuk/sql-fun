use std::str::FromStr;

use sql_fun_core::{PostgresEngineVersion, SqlDialect};
use sql_fun_server_api::EngineDialect;

pub struct CoreDialect(SqlDialect);

impl From<&EngineDialect> for CoreDialect {
    fn from(value: &EngineDialect) -> Self {
        match value {
            EngineDialect::PostgreSQL => Self(SqlDialect::PostgreSQL),
        }
    }
}

impl From<CoreDialect> for SqlDialect {
    fn from(value: CoreDialect) -> Self {
        value.0
    }
}

pub struct CorePostgresVersion(PostgresEngineVersion);

#[derive(Debug)]
pub struct InvalidEngneVersion(String);

impl std::fmt::Display for InvalidEngneVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid version {}", self.0)
    }
}

impl TryFrom<&sql_fun_server_api::EngineVersion> for CorePostgresVersion {
    type Error = InvalidEngneVersion;
    fn try_from(value: &sql_fun_server_api::EngineVersion) -> Result<Self, Self::Error> {
        match value {
            sql_fun_server_api::EngineVersion::PostgreSQL(version) => Ok(Self(
                PostgresEngineVersion::from_str(version).map_err(InvalidEngneVersion)?,
            )),
        }
    }
}

impl From<CorePostgresVersion> for PostgresEngineVersion {
    fn from(value: CorePostgresVersion) -> Self {
        value.0
    }
}
