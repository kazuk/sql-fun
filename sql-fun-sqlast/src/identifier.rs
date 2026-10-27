//! Identifier presentation shared by semantic names and SQL emission.
use std::fmt::{Display, Formatter};

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

pub(crate) struct CanonicalIdentifier<'a>(&'a str);

impl<'a> From<&'a str> for CanonicalIdentifier<'a> {
    fn from(value: &'a str) -> Self {
        Self(value)
    }
}

impl CanonicalIdentifier<'_> {
    fn unquoted(&self) -> bool {
        let mut bytes = self.0.bytes();
        if !matches!(bytes.next(), Some(b'a'..=b'z' | b'_'))
            || !bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'$'))
        {
            return false;
        }
        // Use the locked PostgreSQL parser's keyword categories, rather than
        // maintaining a second keyword list. Unreserved words are safe names.
        let Ok(scan) = ::pg_query::scan(self.0) else {
            return false;
        };
        matches!(scan.tokens.as_slice(), [token] if matches!(
            token.keyword_kind(),
            ::pg_query::protobuf::KeywordKind::NoKeyword
                | ::pg_query::protobuf::KeywordKind::UnreservedKeyword
        ))
    }
}

impl Display for CanonicalIdentifier<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.unquoted() {
            f.write_str(self.0)
        } else {
            Display::fmt(&QuotedIdentifier(self.0), f)
        }
    }
}
