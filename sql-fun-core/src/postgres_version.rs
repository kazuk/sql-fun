use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

/// `PostgreSQL` major and optional beta/RC identity, independent of extraction support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostgresVersion {
    major: u32,
    prerelease: Option<Prerelease>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prerelease {
    Beta(u32),
    Rc(u32),
}

impl Prerelease {
    const fn beta(number: u32) -> Self {
        Self::Beta(number)
    }
    const fn rc(number: u32) -> Self {
        Self::Rc(number)
    }
}
impl fmt::Display for Prerelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Beta(number) => write!(f, "beta{number}"),
            Self::Rc(number) => write!(f, "rc{number}"),
        }
    }
}

impl PostgresVersion {
    const fn stable(major: u32) -> Self {
        Self {
            major,
            prerelease: None,
        }
    }

    /// Numeric `PostgreSQL` major.
    #[must_use]
    pub const fn major(&self) -> u32 {
        self.major
    }

    /// Whether this identifies a stable release.
    #[must_use]
    pub const fn is_stable(&self) -> bool {
        self.prerelease.is_none()
    }

    /// Canonical builtin installation path, also used by analysis.
    #[must_use]
    pub fn definition_base_path(&self, home: &Path) -> PathBuf {
        home.join("postgres").join(self.to_string())
    }

    /// Docker image registered for extraction of this version.
    ///
    /// # Errors
    /// Returns an error if extraction has not been enabled for this identity.
    pub fn extraction_image(&self) -> Result<&'static str, String> {
        PostgresVersionInventory::builtin().image(self)
    }
}

impl FromStr for PostgresVersion {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        let major: u32 = text[..digits]
            .parse()
            .map_err(|_| format!("invalid PostgreSQL version {text}"))?;
        if major < 10 || text.starts_with('0') {
            return Err(format!("invalid PostgreSQL version {text}"));
        }
        let suffix = &text[digits..];
        let prerelease = if suffix.is_empty() {
            None
        } else if let Some(number) = suffix.strip_prefix("beta") {
            Some(Prerelease::beta(parse_prerelease(number, text)?))
        } else if let Some(number) = suffix.strip_prefix("rc") {
            Some(Prerelease::rc(parse_prerelease(number, text)?))
        } else {
            return Err(format!("invalid PostgreSQL version {text}"));
        };
        Ok(Self { major, prerelease })
    }
}

fn parse_prerelease(number: &str, text: &str) -> Result<u32, String> {
    if number.is_empty()
        || !number.bytes().all(|byte| byte.is_ascii_digit())
        || number.starts_with('0')
    {
        return Err(format!("invalid PostgreSQL version {text}"));
    }
    number
        .parse()
        .map_err(|_| format!("invalid PostgreSQL version {text}"))
}

#[cfg(test)]
mod test_parse {
    use super::PostgresVersion;
    #[rstest::rstest]
    #[case::stable("19", 19, true)]
    #[case::future("123", 123, true)]
    #[case::beta("19beta2", 19, false)]
    #[case::rc("19rc1", 19, false)]
    fn round_trips(#[case] text: &str, #[case] major: u32, #[case] stable: bool) {
        let version: PostgresVersion = text.parse().unwrap();
        assert_eq!(version.major(), major);
        assert_eq!(version.is_stable(), stable);
        assert_eq!(version.to_string(), text);
        assert_eq!(
            version.definition_base_path(std::path::Path::new("home")),
            std::path::PathBuf::from(format!("home/postgres/{text}"))
        );
    }
    #[rstest::rstest]
    #[case("9")]
    #[case("019")]
    #[case("19beta0")]
    #[case("19beta")]
    #[case("19/../18")]
    #[case("")]
    fn rejects_invalid_identity(#[case] text: &str) {
        assert!(text.parse::<PostgresVersion>().is_err());
    }
}

impl fmt::Display for PostgresVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.major)?;
        if let Some(release) = self.prerelease {
            write!(f, "{release}")
        } else {
            Ok(())
        }
    }
}

impl serde::Serialize for PostgresVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> serde::Deserialize<'de> for PostgresVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy)]
struct Support {
    version: PostgresVersion,
    image: Option<&'static str>,
}
impl Support {
    #[cfg(test)]
    const fn extractable(version: &PostgresVersion, image: &'static str) -> Self {
        Self {
            version: *version,
            image: Some(image),
        }
    }

    const fn stable(major: u32, image: Option<&'static str>) -> Self {
        Self {
            version: PostgresVersion::stable(major),
            image,
        }
    }
}

// Known older versions remain valid identities without becoming init targets.
const VERSIONS: &[Support] = &[
    Support::stable(15, None),
    Support::stable(16, None),
    Support::stable(
        17,
        Some("postgres@sha256:29e0bb09c8e7e7fc265ea9f4367de9622e55bae6b0b97e7cce740c2d63c2ebc0"),
    ),
    Support::stable(
        18,
        Some("postgres@sha256:5773fe724c49c42a7a9ca70202e11e1dff21fb7235b335a73f39297d200b73a2"),
    ),
];

/// Authoritative `PostgreSQL` extraction capability inventory (catalog format 1).
pub struct PostgresVersionInventory<'a>(&'a [Support]);

impl PostgresVersionInventory<'static> {
    /// Repository-supported version identities and capabilities.
    #[must_use]
    pub const fn builtin() -> Self {
        Self(VERSIONS)
    }
}
impl PostgresVersionInventory<'_> {
    /// All extraction-enabled identities, including explicitly registered prereleases.
    #[must_use]
    pub fn targets(&self) -> Vec<PostgresVersion> {
        self.0
            .iter()
            .filter(|entry| entry.image.is_some())
            .map(|entry| entry.version)
            .collect()
    }

    /// Stable versions available for builtin extraction and initialization.
    #[must_use]
    pub fn stable_targets(&self) -> Vec<PostgresVersion> {
        self.0
            .iter()
            .filter(|entry| entry.image.is_some() && entry.version.is_stable())
            .map(|entry| entry.version)
            .collect()
    }

    /// Validate and normalize an explicit selection in inventory order.
    ///
    /// # Errors
    /// Rejects empty selections and identities lacking extraction support.
    pub fn select(&self, requested: &[String]) -> Result<Vec<PostgresVersion>, String> {
        if requested.is_empty() {
            return Err("select at least one PostgreSQL version".to_owned());
        }
        let mut parsed = Vec::new();
        for text in requested {
            let version = text.parse()?;
            self.image(&version)?;
            parsed.push(version);
        }
        Ok(self
            .0
            .iter()
            .filter(|entry| parsed.contains(&entry.version))
            .map(|entry| entry.version)
            .collect())
    }

    /// Resolve actual server identity before querying version-dependent catalogs.
    ///
    /// # Errors
    /// Rejects unsupported versions, including unregistered beta/RC identities.
    pub fn detect(
        &self,
        version_num: u32,
        server_version: &str,
    ) -> Result<PostgresVersion, String> {
        let major = version_num / 10_000;
        let first = server_version.split_whitespace().next().unwrap_or_default();
        let identity = if first.contains("beta") || first.contains("rc") {
            first.parse::<PostgresVersion>()?
        } else {
            PostgresVersion::stable(major)
        };
        if identity.major() != major {
            return Err("inconsistent PostgreSQL server version".to_owned());
        }
        self.image(&identity)?;
        Ok(identity)
    }

    fn image(&self, version: &PostgresVersion) -> Result<&'static str, String> {
        let image = if let Some(entry) = self.0.iter().find(|entry| entry.version == *version) {
            entry.image
        } else {
            None
        };
        image.ok_or_else(|| format!("PostgreSQL {version} is not supported for builtin extraction; supported stable versions: {}", self.stable_targets().into_iter().map(|version| version.to_string()).collect::<Vec<_>>().join(", ")))
    }
}

#[cfg(test)]
mod test_inventory {
    use super::{PostgresVersion, PostgresVersionInventory, Support, VERSIONS};
    #[rstest::rstest]
    #[case::seventeen(&["17"], &["17"])]
    #[case::eighteen(&["18"], &["18"])]
    #[case::both(&["18", "17", "18"], &["17", "18"])]
    fn normalizes_supported_subsets(#[case] inputs: &[&str], #[case] expected: &[&str]) {
        let inputs = inputs.iter().map(ToString::to_string).collect::<Vec<_>>();
        let result = PostgresVersionInventory::builtin().select(&inputs).unwrap();
        assert_eq!(
            result
                .into_iter()
                .map(|version| version.to_string())
                .collect::<Vec<_>>(),
            expected
        );
    }
    #[rstest::rstest]
    #[case::empty(&[])]
    #[case::older(&["16"])]
    #[case::newer(&["19"])]
    #[case::prerelease(&["19beta1"])]
    fn rejects_unavailable_targets(#[case] inputs: &[&str]) {
        assert!(
            PostgresVersionInventory::builtin()
                .select(&inputs.iter().map(ToString::to_string).collect::<Vec<_>>())
                .is_err()
        );
    }
    #[test]
    fn inventory_extension_drives_selection_detection_and_storage() {
        let mut support = VERSIONS.to_vec();
        support.push(Support::stable(19, Some("postgres:19")));
        let beta: PostgresVersion = "20beta1".parse().unwrap();
        support.push(Support::extractable(&beta, "postgres:20beta1"));
        let inventory = PostgresVersionInventory(&support);
        assert_eq!(
            inventory
                .stable_targets()
                .into_iter()
                .map(|version| version.major())
                .collect::<Vec<_>>(),
            [17, 18, 19]
        );
        assert_eq!(
            inventory.select(&["19".to_owned()]).unwrap()[0].to_string(),
            "19"
        );
        assert_eq!(inventory.select(&["20beta1".to_owned()]).unwrap(), [beta]);
        assert_eq!(inventory.targets().len(), 4);
        assert_eq!(inventory.image(&beta).unwrap(), "postgres:20beta1");
        assert_eq!(inventory.detect(200_000, "20beta1").unwrap(), beta);
        assert_eq!(
            inventory
                .detect(190_002, "19.2 (test)")
                .unwrap()
                .to_string(),
            "19"
        );
        assert!(inventory.detect(210_000, "21.0").is_err());
    }
}
