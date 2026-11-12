use std::fmt::Display;

use ::compact_str::CompactString;

/// part of name specifies catalog
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CatalogName(CompactString);

#[cfg(test)]
mod identity_tests {
    use crate::names::CatalogName;
    use std::{
        collections::{HashSet, hash_map::RandomState},
        hash::BuildHasher,
    };

    #[rstest::rstest]
    #[case::short("postgres".to_owned())]
    #[case::empty(String::new())]
    #[case::quoted(r#"Mixed.name "界""#.to_owned())]
    #[case::postgres_ascii_boundary("x".repeat(63))]
    #[case::postgres_unicode_boundary("界".repeat(21))]
    #[case::long_logical_name("long_identifier".repeat(32))]
    fn clone_preserves_text_and_serialized_identity(
        #[case] text: String,
    ) -> testresult::TestResult {
        let original = CatalogName::from(&text);
        let cloned = original.clone();
        drop(original);
        assert_eq!(cloned.as_str(), text);
        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&cloned), hasher.hash_one(&text));
        let encoded = serde_json::to_value(&cloned)?;
        assert_eq!(encoded, serde_json::Value::String(text));
        let restored = serde_json::from_value::<CatalogName>(encoded)?;
        assert!(HashSet::from([cloned]).contains(&restored));
        Ok(())
    }
}

impl CatalogName {
    /// create from string
    #[must_use]
    pub fn from(value: &str) -> Self {
        Self(CompactString::new(value))
    }
    /// convert to string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for CatalogName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl Display for CatalogName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
