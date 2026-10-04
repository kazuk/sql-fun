use super::error::InitError;

use std::str::FromStr;

use ::sql_fun_core::{PostgresVersion, PostgresVersionInventory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostgresVersions(PostgresVersion);

impl PostgresVersions {
    pub fn all() -> Vec<Self> {
        PostgresVersionInventory::builtin()
            .targets()
            .into_iter()
            .map(Self)
            .collect()
    }
    pub const fn version(&self) -> PostgresVersion {
        self.0
    }
    pub const fn from_version(version: &PostgresVersion) -> Self {
        Self(*version)
    }
}
impl std::fmt::Display for PostgresVersions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PostgreSQL Version {}", self.0)
    }
}
impl FromStr for PostgresVersions {
    type Err = InitError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match PostgresVersionInventory::builtin().select(&[text.to_owned()]) {
            Ok(versions) => Ok(Self(versions[0])),
            Err(_) => InitError::unexpected_version(text),
        }
    }
}
