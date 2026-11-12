use std::{fmt::Display, ops::Deref};

use ::compact_str::CompactString;

/// part of name for schema specific local name
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct LocalName(CompactString);

#[cfg(test)]
mod identity_tests {
    use crate::names::LocalName;
    use std::{
        collections::{HashSet, hash_map::RandomState},
        hash::BuildHasher,
    };

    #[rstest::rstest]
    #[case::short("int4".to_owned())]
    #[case::empty(String::new())]
    #[case::quoted(r#"Mixed.name "界""#.to_owned())]
    #[case::postgres_ascii_boundary("x".repeat(63))]
    #[case::postgres_unicode_boundary("界".repeat(21))]
    #[case::long_logical_name("long_identifier".repeat(32))]
    fn clone_preserves_text_and_serialized_identity(
        #[case] text: String,
    ) -> testresult::TestResult {
        let original = LocalName::from(&text);
        let cloned = original.clone();
        drop(original);
        assert_eq!(cloned.as_str(), text);
        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&cloned), hasher.hash_one(&text));
        let encoded = serde_json::to_value(&cloned)?;
        assert_eq!(encoded, serde_json::Value::String(text));
        let restored = serde_json::from_value::<LocalName>(encoded)?;
        assert!(HashSet::from([cloned]).contains(&restored));
        Ok(())
    }
}

impl LocalName {
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

    /// local name is record
    #[must_use]
    pub fn is_record(&self) -> bool {
        &self.0 == "record"
    }
}

impl Display for LocalName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for LocalName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}
