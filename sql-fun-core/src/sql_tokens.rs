//! Parser-independent SQL lexical data for presentation consumers.
use std::ops::Range;

use crate::SqlDialect;

/// Presentation category of a SQL token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier.
    Identifier,
    /// A literal value or parameter.
    Constant,
    /// An operator.
    Operator,
    /// A keyword.
    Keyword,
    /// A comment.
    Comment,
    /// Whitespace between tokens.
    Trivia,
    /// Punctuation.
    Delimiter,
}

impl TokenKind {
    /// Identifier category.
    #[must_use]
    pub const fn identifier() -> Self {
        Self::Identifier
    }
    /// Constant category.
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
    /// Whether this is an identifier.
    #[must_use]
    pub const fn is_identifier(self) -> bool {
        matches!(self, Self::Identifier)
    }
    /// Whether this is a constant.
    #[must_use]
    pub const fn is_constant(self) -> bool {
        matches!(self, Self::Constant)
    }
    /// Whether this is an operator.
    #[must_use]
    pub const fn is_operator(self) -> bool {
        matches!(self, Self::Operator)
    }
    /// Whether this is a keyword.
    #[must_use]
    pub const fn is_keyword(self) -> bool {
        matches!(self, Self::Keyword)
    }
    /// Whether this is a comment.
    #[must_use]
    pub const fn is_comment(self) -> bool {
        matches!(self, Self::Comment)
    }
    /// Whether this is whitespace.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::Trivia)
    }
    /// Whether this is punctuation.
    #[must_use]
    pub const fn is_delimiter(self) -> bool {
        matches!(self, Self::Delimiter)
    }
}

/// A token's category and byte range in its original SQL source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlToken {
    kind: TokenKind,
    span: Range<usize>,
}

impl SqlToken {
    /// Construct a token; its source bounds are validated by [`TokenStream::new`].
    #[must_use]
    pub const fn new(kind: TokenKind, span: Range<usize>) -> Self {
        Self { kind, span }
    }
    /// Presentation category.
    #[must_use]
    pub const fn kind(&self) -> TokenKind {
        self.kind
    }
    /// Half-open UTF-8 byte range in the original source.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

/// Invalid token bounds, ordering, or uncovered non-whitespace source.
#[derive(Debug, Clone, PartialEq, Eq, ::thiserror::Error)]
#[error("invalid SQL token ranges")]
pub struct TokenStreamError;

/// Lossless, ordered tokens tied to an original source and explicit dialect.
#[derive(Debug, Clone)]
pub struct TokenStream {
    source: String,
    dialect: SqlDialect,
    tokens: Vec<SqlToken>,
}

impl TokenStream {
    /// Validate token bounds and fill omitted whitespace with trivia tokens.
    ///
    /// # Errors
    /// Returns an error for overlapping, empty, non-UTF-8-aligned, or out-of-bounds
    /// ranges, or any uncovered text other than whitespace.
    pub fn new(
        source: &str,
        dialect: SqlDialect,
        tokens: Vec<SqlToken>,
    ) -> Result<Self, TokenStreamError> {
        let mut complete = Vec::new();
        let mut end = 0;
        for token in tokens {
            if token.span.start < end
                || token.span.start >= token.span.end
                || source.get(token.span.clone()).is_none()
            {
                return Err(TokenStreamError);
            }
            Self::append_trivia(source, end..token.span.start, &mut complete)?;
            end = token.span.end;
            complete.push(token);
        }
        Self::append_trivia(source, end..source.len(), &mut complete)?;
        Ok(Self {
            source: source.to_owned(),
            dialect,
            tokens: complete,
        })
    }

    fn append_trivia(
        source: &str,
        span: Range<usize>,
        tokens: &mut Vec<SqlToken>,
    ) -> Result<(), TokenStreamError> {
        let Some(text) = source.get(span.clone()) else {
            return Err(TokenStreamError);
        };
        if !text.chars().all(char::is_whitespace) {
            return Err(TokenStreamError);
        }
        if !text.is_empty() {
            tokens.push(SqlToken::new(TokenKind::trivia(), span));
        }
        Ok(())
    }

    /// Original, unmodified SQL source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Explicitly selected source dialect.
    #[must_use]
    pub const fn dialect(&self) -> SqlDialect {
        self.dialect
    }
    /// Ordered tokens including all whitespace and comments.
    pub fn tokens(&self) -> impl Iterator<Item = &SqlToken> {
        self.tokens.iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        SqlDialect,
        sql_tokens::{SqlToken, TokenKind, TokenStream},
    };

    #[rstest::rstest]
    #[case::empty("")]
    #[case::whitespace(" \r\n\t")]
    fn retains_source_without_semantic_tokens(#[case] source: &str) {
        let stream = TokenStream::new(source, SqlDialect::PostgreSQL, vec![]).unwrap();
        let reconstructed: String = stream
            .tokens()
            .map(|token| &stream.source()[token.span()])
            .collect();
        assert_eq!(reconstructed, source);
    }

    #[test]
    fn retains_original_utf8_and_all_trivia() {
        let source = "  名\r\n";
        let stream = TokenStream::new(
            source,
            SqlDialect::PostgreSQL,
            vec![SqlToken::new(TokenKind::identifier(), 2..5)],
        )
        .unwrap();
        let reconstructed: String = stream
            .tokens()
            .map(|token| &stream.source()[token.span()])
            .collect();
        assert_eq!(reconstructed, source);
        assert!(stream.tokens().next().unwrap().kind().is_trivia());
        assert_eq!(stream.dialect(), SqlDialect::PostgreSQL);
    }

    #[rstest::rstest]
    #[case::split_utf8(1..3)]
    #[case::out_of_bounds(0..9)]
    #[case::empty(0..0)]
    fn rejects_invalid_ranges(#[case] span: std::ops::Range<usize>) {
        assert!(
            TokenStream::new(
                "名",
                SqlDialect::PostgreSQL,
                vec![SqlToken::new(TokenKind::identifier(), span)]
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_overlapping_and_uncovered_tokens() {
        let token = SqlToken::new(TokenKind::identifier(), 0..1);
        assert!(TokenStream::new("a", SqlDialect::PostgreSQL, vec![token.clone(), token]).is_err());
        assert!(TokenStream::new("a", SqlDialect::PostgreSQL, vec![]).is_err());
    }
}
