//! Parsing SQL text into owned common statements.

mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

pub(crate) use self::postgres::split_postgres_statements;

use crate::StringSpan;
use crate::ast::{Dialect, Statement};
use crate::sem::AnalysisError;

/// A failed statement conversion or a syntax error that ends parsing.
///
/// The span identifies the attempted input, in UTF-8 byte offsets. It need not
/// identify an individual token: parsers differ in their diagnostic precision.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ParseStatementError {
    message: String,
    span: StringSpan,
    syntax: bool,
    position: Option<StringSpan>,
}

impl ParseStatementError {
    pub(crate) fn syntax(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            syntax: true,
            position: None,
        }
    }

    pub(crate) fn conversion(span: StringSpan, error: &AnalysisError) -> Self {
        Self::unsupported(span, &error.to_string())
    }

    pub(crate) fn unsupported(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            syntax: false,
            position: None,
        }
    }

    #[cfg(feature = "sqlite")]
    pub(crate) fn with_position(mut self, position: StringSpan) -> Self {
        self.position = Some(position);
        self
    }

    /// Range of the statement or input whose parsing failed.
    pub const fn span(&self) -> &StringSpan {
        &self.span
    }

    /// More precise source range reported by the parser, when available.
    ///
    /// Offsets refer to the complete input, not to an isolated statement slice.
    pub const fn position(&self) -> Option<&StringSpan> {
        self.position.as_ref()
    }

    /// Whether a parser failure prevented further statement boundary discovery.
    pub const fn is_syntax(&self) -> bool {
        self.syntax
    }

    /// Diagnostic supplied by the parser or common AST converter.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Parse SQL in the explicitly selected dialect, yielding each statement's result.
///
/// Names remain unresolved and no catalog, configuration, or files are read.
/// Conversion errors with known statement boundaries allow iteration to continue.
/// A syntax error is yielded once and ends iteration. Exhaust the iterator to
/// check the entire input; an empty input yields no statements.
///
/// PostgreSQL discovers boundaries for the complete input before converting
/// statements. A syntax failure there yields one error for the input, with no
/// preceding successes. SQLite discovers boundaries incrementally.
pub fn parse_statements(
    sql: &str,
    dialect: Dialect,
) -> impl Iterator<Item = Result<Statement, ParseStatementError>> + '_ {
    let statements: Box<dyn Iterator<Item = Result<Statement, ParseStatementError>> + '_> =
        if dialect == Dialect::postgres() {
            Box::new(self::postgres::parse(sql))
        } else {
            #[cfg(feature = "sqlite")]
            {
                Box::new(self::sqlite::parse(sql))
            }
            #[cfg(not(feature = "sqlite"))]
            {
                Box::new(std::iter::once(Err(ParseStatementError::unsupported(
                    StringSpan::new(0, sql.len()),
                    "SQLite support requires the sqlite feature",
                ))))
            }
        };
    statements
}
