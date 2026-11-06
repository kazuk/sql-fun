//! Syntax of explicitly supplied client parameter type hints.
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;
mod token;

use self::token::TypeTokens;
use crate::{ast::Dialect, error::AnalysisError};

/// Parser-independent type-name syntax for a client parameter hint.
pub(crate) struct ClientTypeHint {
    name: ClientTypeName,
    array: bool,
}

/// Parsed spelling category; resolution belongs to semantic analysis.
pub(crate) enum ClientTypeName {
    Named(ClientTypePath),
    Builtin(ClientBuiltin),
    Float(Option<u32>),
    #[cfg(feature = "sqlite")]
    Sqlite(Vec<String>),
    UnsupportedModifier,
}

/// A syntactically valid qualified type name.
pub(crate) enum ClientTypePath {
    Local(String),
    Schema(String, String),
    Catalog(String, String, String),
}

/// Built-in type spellings recognized by the hint grammar.
pub(crate) enum ClientBuiltin {
    Integer,
    Smallint,
    Bigint,
    Real,
    Boolean,
    Numeric,
    DoublePrecision,
    Character,
    Varchar,
    Bit,
    BitVarying,
    Time,
    TimeWithZone,
    Timestamp,
    TimestampWithZone,
    Interval,
    Json,
}

impl ClientTypeHint {
    const fn new(name: ClientTypeName, array: bool) -> Self {
        Self { name, array }
    }
    const fn unsupported_modifier() -> Self {
        Self::new(ClientTypeName::UnsupportedModifier, false)
    }
    pub(crate) fn into_parts(self) -> (ClientTypeName, bool) {
        (self.name, self.array)
    }

    pub(crate) fn parse(input: &str, dialect: Dialect) -> Result<Self, AnalysisError> {
        if input.contains('\0') {
            return Err(invalid("NUL in type name"));
        }
        if dialect == Dialect::postgres() {
            postgres::parse(&TypeTokens::postgres(input)?)
        } else {
            #[cfg(feature = "sqlite")]
            {
                sqlite::parse(&TypeTokens::sqlite(input)?)
            }
            #[cfg(not(feature = "sqlite"))]
            {
                Err(invalid("SQLite requires the sqlite feature"))
            }
        }
    }
}

fn invalid(detail: &str) -> AnalysisError {
    AnalysisError::unexpected_input_error(&format!("SQL type name: {detail}"))
}
