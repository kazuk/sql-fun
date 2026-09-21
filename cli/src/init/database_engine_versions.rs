use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostgresVersions {
    Postgres16,
    Postgres17,
    Postgres18,
}

impl PostgresVersions {
    pub const ALL: [Self; 3] = [Self::Postgres16, Self::Postgres17, Self::Postgres18];
}

impl std::fmt::Display for PostgresVersions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let version = match self {
            Self::Postgres16 => 16,
            Self::Postgres17 => 17,
            Self::Postgres18 => 18,
        };
        write!(
            f,
            "{}",
            format!(include_str!("postgres_version.txt"), version = version).trim_end()
        )
    }
}

impl FromStr for PostgresVersions {
    type Err = anyhow::Error;

    fn from_str(version: &str) -> Result<Self, Self::Err> {
        match version {
            "16" => Ok(Self::Postgres16),
            "17" => Ok(Self::Postgres17),
            "18" => Ok(Self::Postgres18),
            _ => anyhow::bail!(
                format!(include_str!("unexpected_version.txt"), version = version)
                    .trim_end()
                    .to_owned()
            ),
        }
    }
}
