use turso_parser::ast::Literal as LiteralIn;

/// numeric literal
#[derive(Debug)]
pub struct NumericLiteral(String);

impl NumericLiteral {
    /// literal value as text
    pub fn value_text(&self) -> &str {
        self.0.as_str()
    }
}

/// string literal
#[derive(Debug)]
pub struct StringLiteral(String);

impl StringLiteral {
    /// literal value
    pub fn value(&self) -> &str {
        self.0.as_str()
    }
}
/// blob literal
#[derive(Debug)]
pub struct BlobLiteral(String);

impl BlobLiteral {
    /// value
    pub fn value(&self) -> &str {
        self.0.as_str()
    }
}

/// keyword literal
#[derive(Debug)]
pub struct KeywordLiteral(String);

impl KeywordLiteral {
    /// value
    pub fn value(&self) -> &str {
        self.0.as_str()
    }
}

/// null literal
#[derive(Debug)]
pub struct NullLiteral;

/// boolean literal
#[derive(Debug)]
pub struct BooleanLiteral(bool);

impl BooleanLiteral {
    /// literal value
    pub fn value(&self) -> bool {
        self.0
    }
}

/// timestamp
#[derive(Debug)]
pub struct TimestampLiteral(TimestampKind);

impl TimestampLiteral {
    /// get [`TimestampKind`]
    pub fn kind(&self) -> &TimestampKind {
        &self.0
    }
}

/// timestamp kind
#[derive(Debug)]
pub enum TimestampKind {
    /// date only
    Date,
    /// time only
    Time,
    /// date with time
    Timestamp,
}

/// literal
#[derive(Debug)]
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
