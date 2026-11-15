//! Identifier presentation shared by semantic names and SQL emission.
use std::fmt::{Display, Formatter};

use ::compact_str::CompactString;

pub(crate) struct QuotedIdentifier<'a>(&'a str);

impl<'a> From<&'a str> for QuotedIdentifier<'a> {
    fn from(value: &'a str) -> Self {
        Self(value)
    }
}

impl Display for QuotedIdentifier<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.0.replace('"', "\"\""))
    }
}

/// Logical identifier with an initialization-time canonical quotation decision.
/// The derived flag is neither name identity nor serialized state.
#[derive(Clone, serde::Serialize)]
#[serde(transparent)]
pub(crate) struct CanonicalIdentifier {
    text: CompactString,
    #[serde(skip)]
    quoted: bool,
}

impl From<&str> for CanonicalIdentifier {
    fn from(value: &str) -> Self {
        let mut bytes = value.bytes();
        let unquoted = matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
            && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'$'));
        Self {
            text: CompactString::new(value),
            quoted: !unquoted,
        }
    }
}

#[cfg(test)]
mod initialization_tests {
    use crate::identifier::CanonicalIdentifier;

    #[rstest::rstest]
    #[case::plain("simple_name", false)]
    #[case::keyword("select", false)]
    #[case::postgres_keyword("ilike", false)]
    #[case::sqlite_keyword("glob", false)]
    #[case::dollar("_a1$", false)]
    #[case::mixed("Mixed", true)]
    #[case::dot("a.b", true)]
    #[case::quote("a\"b", true)]
    #[case::empty("", true)]
    #[case::numeric("1a", true)]
    #[case::unicode("界", true)]
    #[case::array("item[]", true)]
    fn records_canonical_quotation(#[case] text: &str, #[case] quoted: bool) {
        let identifier = CanonicalIdentifier::from(text);
        assert_eq!(identifier.as_str(), text);
        assert_eq!(identifier.quoted, quoted);
        assert_eq!(identifier.clone().quoted, quoted);
    }
}

impl CanonicalIdentifier {
    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

impl PartialEq for CanonicalIdentifier {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for CanonicalIdentifier {}

impl std::hash::Hash for CanonicalIdentifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.text, state);
    }
}

impl std::fmt::Debug for CanonicalIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.text, f)
    }
}

impl<'de> serde::Deserialize<'de> for CanonicalIdentifier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <CompactString as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self::from(text.as_str()))
    }
}

#[cfg(test)]
mod deserialize_tests {
    use crate::identifier::CanonicalIdentifier;
    use std::hash::{BuildHasher, RandomState};

    #[rstest::rstest]
    #[case::plain("plain", "plain")]
    #[case::keyword("select", "select")]
    #[case::quoted("a.b", r#""a.b""#)]
    #[case::escaped("a\"b", r#""a""b""#)]
    fn restores_presentation_without_changing_wire_identity(
        #[case] text: &str,
        #[case] canonical: &str,
    ) -> testresult::TestResult {
        let original = CanonicalIdentifier::from(text);
        let json = ::serde_json::to_value(&original)?;
        assert_eq!(json, ::serde_json::Value::String(text.to_owned()));
        let restored: CanonicalIdentifier = ::serde_json::from_value(json)?;
        assert_eq!(restored, original);
        assert_eq!(restored.to_string(), canonical);
        let hasher = RandomState::new();
        assert_eq!(hasher.hash_one(&restored), hasher.hash_one(text));
        Ok(())
    }
}

impl Display for CanonicalIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.quoted {
            Display::fmt(&QuotedIdentifier(self.as_str()), f)
        } else {
            f.write_str(self.as_str())
        }
    }
}
