use turso_parser::ast::Literal as LiteralIn;

/// numeric literal
#[derive(Clone, Debug)]
pub struct NumericLiteral(String);

impl NumericLiteral {
    /// literal value as text
    pub const fn value_text(&self) -> &str {
        self.0.as_str()
    }
}

/// string literal
#[derive(Clone, Debug)]
pub struct StringLiteral(String);

impl StringLiteral {
    /// literal value
    pub const fn value(&self) -> &str {
        self.0.as_str()
    }
}
/// blob literal
#[derive(Clone, Debug)]
pub struct BlobLiteral(String);

impl BlobLiteral {
    /// Hexadecimal payload for semantic consumers.
    pub fn value(&self) -> &str {
        ::turso_parser::ast::blob_literal_hex(&self.0)
    }

    /// Complete parser-provided spelling, including prefix and quotes.
    pub const fn spelling(&self) -> &str {
        self.0.as_str()
    }
}

/// keyword literal
#[derive(Clone, Debug)]
pub struct KeywordLiteral(String);

impl KeywordLiteral {
    /// value
    pub const fn value(&self) -> &str {
        self.0.as_str()
    }
}

/// null literal
#[derive(Clone, Debug)]
pub struct NullLiteral;

/// boolean literal
#[derive(Clone, Debug)]
pub struct BooleanLiteral(bool);

impl BooleanLiteral {
    /// literal value
    pub const fn value(&self) -> bool {
        self.0
    }
}

/// timestamp
#[derive(Clone, Debug)]
pub struct TimestampLiteral(TimestampKind);

impl TimestampLiteral {
    /// get [`TimestampKind`]
    pub const fn kind(&self) -> &TimestampKind {
        &self.0
    }
}

/// timestamp kind
#[derive(Clone, Debug)]
pub enum TimestampKind {
    /// date only
    Date,
    /// time only
    Time,
    /// date with time
    Timestamp,
}

/// literal
#[derive(Clone, Debug)]
pub enum Literal {
    /// numeric literal
    Numeric(NumericLiteral),
    /// string  literal
    String(StringLiteral),
    /// blob literal
    Blob(BlobLiteral),
    /// keyword
    Keyword(KeywordLiteral),
    /// null
    Null(NullLiteral),
    /// true
    True(BooleanLiteral),
    /// false
    False(BooleanLiteral),
    /// current date literal
    CurrentDate(TimestampLiteral),
    /// current time literal
    CurrentTime(TimestampLiteral),
    /// current timestamp literal
    CurrentTimestamp(TimestampLiteral),
}

impl Literal {
    /// Numeric literal spelling, if present.
    pub const fn numeric_text(&self) -> Option<&str> {
        if let Self::Numeric(value) = self {
            Some(value.value_text())
        } else {
            None
        }
    }
}

impl From<LiteralIn> for Literal {
    fn from(value: LiteralIn) -> Self {
        match value {
            LiteralIn::Numeric(v) => Self::Numeric(NumericLiteral(v)),
            LiteralIn::String(v) => Self::String(StringLiteral(v)),
            LiteralIn::Blob(v) => Self::Blob(BlobLiteral(v)),
            LiteralIn::Keyword(v) => Self::Keyword(KeywordLiteral(v)),
            LiteralIn::Null => Self::Null(NullLiteral),
            LiteralIn::True => Self::True(BooleanLiteral(true)),
            LiteralIn::False => Self::False(BooleanLiteral(false)),
            LiteralIn::CurrentDate => Self::CurrentDate(TimestampLiteral(TimestampKind::Date)),
            LiteralIn::CurrentTime => Self::CurrentTime(TimestampLiteral(TimestampKind::Time)),
            LiteralIn::CurrentTimestamp => {
                Self::CurrentTimestamp(TimestampLiteral(TimestampKind::Timestamp))
            }
        }
    }
}

#[cfg(test)]
mod test_parser_blob_payload {
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::Expr;
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::literal::Literal;
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;

    #[rstest::rstest]
    #[case::lowercase("SELECT x'ab'", "ab")]
    #[case::uppercase("SELECT X'AB'", "AB")]
    #[case::mixed_case("SELECT x'aBcD'", "aBcD")]
    #[case::empty_lowercase("SELECT x''", "")]
    #[case::empty_uppercase("SELECT X''", "")]
    fn preserves_parser_spelling_and_hex_payload(#[case] sql: &str, #[case] expected: &str) {
        let Expr::Literal(Literal::Blob(blob)) = SqliteSynBuilder.parse_expression(sql) else {
            panic!("expected BLOB: {sql}");
        };
        assert_eq!(blob.value(), expected);
        assert_eq!(blob.spelling(), &sql[7..]);
    }
}
