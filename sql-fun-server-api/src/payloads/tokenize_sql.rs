//! Presentation tokens transported without repeating the original SQL.
use ::sql_fun_core::{SqlDialect, SqlToken as CoreToken, TokenKind as CoreKind};

/// Original source and explicitly selected lexer. No schema context is needed.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TokenizeSqlArgs {
    source: String,
    dialect: SqlDialect,
}

impl TokenizeSqlArgs {
    /// Retain the exact SQL bytes and the caller's explicit dialect.
    #[must_use]
    pub fn new(source: &str, dialect: SqlDialect) -> Self {
        Self {
            source: source.to_owned(),
            dialect,
        }
    }
    /// Original SQL, without normalization.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Explicit lexer selection.
    #[must_use]
    pub const fn dialect(&self) -> SqlDialect {
        self.dialect
    }
}

/// Parser-independent presentation category on the wire.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCategory {
    /// Identifier.
    Identifier,
    /// Literal or parameter.
    Constant,
    /// Operator.
    Operator,
    /// Keyword.
    Keyword,
    /// Comment, including its delimiters.
    Comment,
    /// Whitespace.
    Trivia,
    /// Punctuation.
    Delimiter,
}

impl TokenCategory {
    /// Identifier category.
    #[must_use]
    pub const fn identifier() -> Self {
        Self::Identifier
    }
    /// Literal or parameter category.
    #[must_use]
    pub const fn constant() -> Self {
        Self::Constant
    }
    /// Operator category.
    #[must_use]
    pub const fn operator() -> Self {
        Self::Operator
    }
    /// Keyword category.
    #[must_use]
    pub const fn keyword() -> Self {
        Self::Keyword
    }
    /// Comment category.
    #[must_use]
    pub const fn comment() -> Self {
        Self::Comment
    }
    /// Whitespace category.
    #[must_use]
    pub const fn trivia() -> Self {
        Self::Trivia
    }
    /// Punctuation category.
    #[must_use]
    pub const fn delimiter() -> Self {
        Self::Delimiter
    }
}

impl From<CoreKind> for TokenCategory {
    fn from(value: CoreKind) -> Self {
        match value {
            CoreKind::Identifier => Self::identifier(),
            CoreKind::Constant => Self::constant(),
            CoreKind::Operator => Self::operator(),
            CoreKind::Keyword => Self::keyword(),
            CoreKind::Comment => Self::comment(),
            CoreKind::Trivia => Self::trivia(),
            CoreKind::Delimiter => Self::delimiter(),
        }
    }
}
impl From<TokenCategory> for CoreKind {
    fn from(value: TokenCategory) -> Self {
        match value {
            TokenCategory::Identifier => Self::identifier(),
            TokenCategory::Constant => Self::constant(),
            TokenCategory::Operator => Self::operator(),
            TokenCategory::Keyword => Self::keyword(),
            TokenCategory::Comment => Self::comment(),
            TokenCategory::Trivia => Self::trivia(),
            TokenCategory::Delimiter => Self::delimiter(),
        }
    }
}

/// Category and half-open UTF-8 byte range in the request's original source.
/// Collections contain all tokens in order, including comments and whitespace.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SqlToken {
    kind: TokenCategory,
    start: usize,
    end: usize,
}
impl From<&CoreToken> for SqlToken {
    fn from(value: &CoreToken) -> Self {
        let span = value.span();
        Self {
            kind: value.kind().into(),
            start: span.start,
            end: span.end,
        }
    }
}
impl From<&SqlToken> for CoreToken {
    fn from(value: &SqlToken) -> Self {
        Self::new(value.kind.into(), value.start..value.end)
    }
}
impl super::CollectionTraversal for SqlToken {}

#[cfg(test)]
mod tests {
    use crate::payloads::tokenize_sql::{SqlToken, TokenizeSqlArgs};
    use ::sql_fun_core::{SqlDialect, SqlToken as CoreToken, TokenKind};

    #[rstest::rstest]
    fn round_trips_categories_and_byte_ranges(
        #[values(
            TokenKind::identifier(),
            TokenKind::constant(),
            TokenKind::operator(),
            TokenKind::keyword(),
            TokenKind::comment(),
            TokenKind::trivia(),
            TokenKind::delimiter()
        )]
        kind: TokenKind,
    ) -> ::testresult::TestResult {
        let token = CoreToken::new(kind, 2..8);
        let json = ::serde_json::to_value(SqlToken::from(&token))?;
        assert_eq!(json["start"], 2);
        assert_eq!(json["end"], 8);
        assert_eq!(json.as_object().unwrap().len(), 3);
        let received: SqlToken = ::serde_json::from_value(json)?;
        assert_eq!(CoreToken::from(&received), token);
        Ok(())
    }

    #[test]
    fn retains_source_and_explicit_dialect() -> ::testresult::TestResult {
        let args = TokenizeSqlArgs::new(" SELECT '日本語'; ", SqlDialect::PostgreSQL);
        let json = ::serde_json::to_value(&args)?;
        assert_eq!(
            json,
            ::serde_json::json!({"source":" SELECT '日本語'; ","dialect":"postgres"})
        );
        let received: TokenizeSqlArgs = ::serde_json::from_value(json)?;
        assert_eq!(received.source(), args.source());
        assert_eq!(received.dialect(), args.dialect());
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing_source(r#"{"dialect":"postgres"}"#)]
    #[case::missing_dialect(r#"{"source":"SELECT 1"}"#)]
    #[case::unknown_dialect(r#"{"source":"SELECT 1","dialect":"unknown"}"#)]
    fn rejects_missing_or_unknown_selection(#[case] json: &str) {
        assert!(::serde_json::from_str::<TokenizeSqlArgs>(json).is_err());
    }

    #[cfg(feature = "openrpc-schema")]
    #[test]
    fn schema_requires_source_and_dialect() {
        let schema = ::serde_json::to_value(::schemars::schema_for!(TokenizeSqlArgs)).unwrap();
        assert_eq!(
            schema["required"],
            ::serde_json::json!(["source", "dialect"])
        );
    }
}
