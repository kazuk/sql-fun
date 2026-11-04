//! Dialect lexer adapters for presentation tokens, independent of semantic scans.
use ::sql_fun_core::{SqlDialect, SqlToken, TokenKind, TokenStream, TokenStreamError};

/// Failure to tokenize source in the explicitly selected dialect.
#[derive(Debug, ::thiserror::Error)]
#[error("cannot tokenize SQL: {0}")]
pub struct TokenizeSqlError(String);

impl From<TokenStreamError> for TokenizeSqlError {
    fn from(value: TokenStreamError) -> Self {
        Self(value.to_string())
    }
}

/// Tokenize original SQL using the selected dialect's lexer.
///
/// This presentation stream does not replace the semantic analyzer's token data.
///
/// # Errors
/// Returns lexical failures and invalid lexer source ranges. No dialect fallback
/// or rescanning of partial source is performed.
pub fn tokenize_sql(source: &str, dialect: SqlDialect) -> Result<TokenStream, TokenizeSqlError> {
    let tokens = match dialect {
        SqlDialect::PostgreSQL => postgres_tokens(source)?,
        #[cfg(feature = "sqlite")]
        SqlDialect::SQLite => sqlite_tokens(source)?,
    };
    Ok(TokenStream::new(source, dialect, tokens)?)
}

#[cfg(test)]
mod tests {
    use crate::sql_tokens::tokenize_sql;
    use ::sql_fun_core::SqlDialect;
    #[cfg(feature = "sqlite")]
    use ::sql_fun_core::TokenKind;

    #[test]
    fn postgres_preserves_utf8_comments_and_multiline_literals() {
        let source = concat!(
            "  SELECT ",
            r#""名前", $tag$first"#,
            "\r\n",
            r#"second$tag$, $1 /* comment */;"#,
            "\r\n-- end\n"
        );
        let stream = tokenize_sql(source, SqlDialect::PostgreSQL).unwrap();
        let reconstructed: String = stream
            .tokens()
            .map(|token| &stream.source()[token.span()])
            .collect();
        assert_eq!(source, reconstructed);
        assert!(stream.tokens().any(|token| token.kind().is_comment()));
        assert!(stream.tokens().any(|token| token.kind().is_constant()
            && stream.source()[token.span()].starts_with("$tag$")));
    }

    #[rstest::rstest]
    #[case::unicode_identifier(r#"SELECT U&"d\0061t" UESCAPE '\'"#)]
    #[case::escaped_string(r#"SELECT E'first\nsecond', U&'d\0061t'"#)]
    #[case::quoted_identifiers(r#"SELECT "with space", "double""quote";"#)]
    #[case::nested_comments("SELECT 1 /* outer /* inner */ end */;")]
    fn postgres_retains_quoted_and_dialect_specific_forms(#[case] source: &str) {
        let stream = tokenize_sql(source, SqlDialect::PostgreSQL).unwrap();
        let reconstructed: String = stream
            .tokens()
            .map(|token| &stream.source()[token.span()])
            .collect();
        assert_eq!(reconstructed, source);
    }

    #[test]
    fn lexical_failure_is_returned_without_panicking() {
        assert!(tokenize_sql("SELECT 'unfinished", SqlDialect::PostgreSQL).is_err());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_uses_its_own_quoting_parameters_and_blob_tokens() {
        let source = concat!(
            r#"SELECT [名前], `odd`, "quoted", 'string', X'ab', :parameter, ?1, 6 / 2 - 1 /* multi"#,
            "\n",
            "line */; -- end\n"
        );
        let stream = tokenize_sql(source, SqlDialect::SQLite).unwrap();
        let reconstructed: String = stream
            .tokens()
            .map(|token| &stream.source()[token.span()])
            .collect();
        assert_eq!(source, reconstructed);
        assert!(
            stream
                .tokens()
                .any(|token| token.kind() == TokenKind::constant()
                    && &stream.source()[token.span()] == "X'ab'")
        );
        assert!(
            stream
                .tokens()
                .any(|token| token.kind().is_identifier()
                    && &stream.source()[token.span()] == "`odd`")
        );
        assert!(
            stream.tokens().any(|token| token.kind().is_identifier()
                && &stream.source()[token.span()] == r#""quoted""#)
        );
        assert!(stream.tokens().any(
            |token| token.kind().is_constant() && &stream.source()[token.span()] == "'string'"
        ));
        // The existing Turso lexer exposes brackets separately for its array
        // syntax. Preserve their bytes and categories without reinterpreting
        // that lexer into a different semantic tokenizer.
        assert!(
            stream
                .tokens()
                .any(|token| token.kind().is_delimiter() && &stream.source()[token.span()] == "[")
        );
        assert!(
            stream
                .tokens()
                .any(|token| token.kind().is_identifier()
                    && &stream.source()[token.span()] == "名前")
        );
        assert!(
            stream
                .tokens()
                .any(|token| token.kind().is_delimiter() && &stream.source()[token.span()] == "]")
        );
        assert!(stream.tokens().any(|token| token.kind().is_comment()));
    }
}

fn postgres_tokens(source: &str) -> Result<Vec<SqlToken>, TokenizeSqlError> {
    let scan = ::pg_query::scan(source).map_err(|error| TokenizeSqlError(error.to_string()))?;
    let mut tokens = Vec::new();
    for token in scan.tokens {
        let start =
            usize::try_from(token.start).map_err(|error| TokenizeSqlError(error.to_string()))?;
        let end =
            usize::try_from(token.end).map_err(|error| TokenizeSqlError(error.to_string()))?;
        tokens.push(SqlToken::new(postgres_kind(token.token()), start..end));
    }
    Ok(tokens)
}

const fn postgres_kind(kind: ::pg_query::protobuf::Token) -> TokenKind {
    use ::pg_query::protobuf::Token;
    match kind {
        Token::Ident | Token::Uident => TokenKind::identifier(),
        Token::SqlComment | Token::CComment => TokenKind::comment(),
        Token::Fconst
        | Token::Sconst
        | Token::Usconst
        | Token::Bconst
        | Token::Xconst
        | Token::Iconst
        | Token::Param => TokenKind::constant(),
        Token::Op
        | Token::Ascii37
        | Token::Ascii42
        | Token::Ascii43
        | Token::Ascii45
        | Token::Ascii47
        | Token::Ascii60
        | Token::Ascii61
        | Token::Ascii62
        | Token::Ascii63
        | Token::Ascii94
        | Token::Typecast
        | Token::ColonEquals
        | Token::EqualsGreater
        | Token::LessEquals
        | Token::GreaterEquals
        | Token::NotEquals => TokenKind::operator(),
        Token::Ascii36
        | Token::Ascii40
        | Token::Ascii41
        | Token::Ascii44
        | Token::Ascii46
        | Token::Ascii58
        | Token::Ascii59
        | Token::Ascii91
        | Token::Ascii92
        | Token::Ascii93
        | Token::DotDot => TokenKind::delimiter(),
        _ => TokenKind::keyword(),
    }
}

#[cfg(feature = "sqlite")]
fn sqlite_tokens(source: &str) -> Result<Vec<SqlToken>, TokenizeSqlError> {
    #[cfg(feature = "workaround_turso_9791")]
    #[cfg(feature = "sqlite")]
    use crate::sqlite_parser::Lexer;
    #[cfg(not(feature = "workaround_turso_9791"))]
    use ::turso_parser::lexer::Lexer;

    let mut tokens = Vec::new();
    let mut start = 0usize;
    for token in Lexer::new(source.as_bytes()) {
        let token = token.map_err(|error| TokenizeSqlError(error.to_string()))?;
        // The lexer returns every byte, but punctuation values can be static
        // byte strings rather than slices into the source. Accumulate lengths
        // and verify the original bytes instead of relying on pointer identity.
        let end = start
            .checked_add(token.value.len())
            .ok_or_else(|| TokenizeSqlError("token range overflow".to_owned()))?;
        let text = source
            .get(start..end)
            .ok_or_else(|| TokenizeSqlError("invalid SQLite token range".to_owned()))?;
        if text.as_bytes() != token.value {
            return Err(TokenizeSqlError(
                "SQLite token differs from original SQL".to_owned(),
            ));
        }
        tokens.push(SqlToken::new(
            sqlite_kind(token.token_type, text),
            start..end,
        ));
        start = end;
    }
    Ok(tokens)
}

#[cfg(feature = "sqlite")]
fn sqlite_kind(kind: ::turso_parser::token::TokenType, text: &str) -> TokenKind {
    use ::turso_parser::token::TokenType;
    match kind {
        TokenType::TK_NONE => {
            if text.starts_with("--") || text.starts_with("/*") {
                TokenKind::comment()
            } else {
                TokenKind::trivia()
            }
        }
        TokenType::TK_ID => TokenKind::identifier(),
        TokenType::TK_STRING
        | TokenType::TK_BLOB
        | TokenType::TK_FLOAT
        | TokenType::TK_INTEGER
        | TokenType::TK_VARIABLE => TokenKind::constant(),
        TokenType::TK_SEMI
        | TokenType::TK_LP
        | TokenType::TK_RP
        | TokenType::TK_COMMA
        | TokenType::TK_DOT
        | TokenType::TK_LBRACKET
        | TokenType::TK_RBRACKET
        | TokenType::TK_COLON => TokenKind::delimiter(),
        TokenType::TK_NE
        | TokenType::TK_EQ
        | TokenType::TK_GT
        | TokenType::TK_LE
        | TokenType::TK_LT
        | TokenType::TK_GE
        | TokenType::TK_BITAND
        | TokenType::TK_BITOR
        | TokenType::TK_LSHIFT
        | TokenType::TK_RSHIFT
        | TokenType::TK_PLUS
        | TokenType::TK_MINUS
        | TokenType::TK_STAR
        | TokenType::TK_SLASH
        | TokenType::TK_REM
        | TokenType::TK_CONCAT
        | TokenType::TK_PTR
        | TokenType::TK_BITNOT
        | TokenType::TK_ARRAY_CONTAINS
        | TokenType::TK_ARRAY_OVERLAP => TokenKind::operator(),
        _ => TokenKind::keyword(),
    }
}
