use std::str::FromStr;

use anyhow::bail;
use inquire::{list_option::ListOption, validator::Validation};

#[derive(Clone, Copy, PartialEq)]
pub enum PostgresVersions {
    Postgres16,
    Postgres17,
    Postgres18,
}

impl std::fmt::Display for PostgresVersions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Postgres16 => write!(f, "PostgreSQL Version 16"),
            Self::Postgres17 => write!(f, "PostgreSQL Version 17"),
            Self::Postgres18 => write!(f, "PostgreSQL Version 18"),
        }
    }
}

impl FromStr for PostgresVersions {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "16" => Ok(Self::Postgres16),
            "17" => Ok(Self::Postgres17),
            "18" => Ok(Self::Postgres18),
            _ => bail!("unexpected version {s}"),
        }
    }
}

const ALL_SUPPORTED_VERSIONS: [PostgresVersions; 3] = [
    PostgresVersions::Postgres16,
    PostgresVersions::Postgres17,
    PostgresVersions::Postgres18,
];

#[expect(clippy::unnecessary_wraps)]
fn postgres_engine_selection_validator(
    items: &[ListOption<&PostgresVersions>],
) -> Result<Validation, Box<dyn std::error::Error + Send + Sync>> {
    if items.is_empty() {
        Ok(Validation::Invalid("require select one".into()))
    } else {
        Ok(Validation::Valid)
    }
}

pub fn select_engine_version(
    yes: bool,
    versions: Vec<String>,
) -> Result<Vec<PostgresVersions>, anyhow::Error> {
    let message = include_str!("database_engine_versions.txt");
    println!("{message}");
    let options = ALL_SUPPORTED_VERSIONS.to_vec();
    let versions = versions
        .into_iter()
        .map(|s| PostgresVersions::from_str(&s))
        .collect::<Result<Vec<_>, _>>()?;
    if yes {
        return Ok(versions);
    }

    let mut defaults = Vec::new();
    for (index, value) in ALL_SUPPORTED_VERSIONS.iter().enumerate() {
        if versions.contains(value) {
            defaults.push(index);
        }
    }

    let versions = inquire::MultiSelect::new("Database Engine Versions", options)
        .with_all_selected_by_default()
        .with_validator(postgres_engine_selection_validator)
        .with_default(&defaults)
        .prompt()?;

    Ok(versions)
}

#[cfg(test)]
mod test_select_engine_version {
    use super::{PostgresVersions, select_engine_version};

    #[test]
    fn assumed_yes_preserves_order_and_duplicates() {
        let selected = select_engine_version(
            true,
            vec!["18".to_owned(), "16".to_owned(), "18".to_owned()],
        )
        .unwrap();
        assert!(
            selected
                == vec![
                    PostgresVersions::Postgres18,
                    PostgresVersions::Postgres16,
                    PostgresVersions::Postgres18,
                ]
        );
    }

    #[test]
    fn rejects_unknown_versions_before_prompting() {
        for yes in [false, true] {
            assert!(select_engine_version(yes, vec!["19".to_owned()]).is_err());
        }
    }
}
