//! Parsing SQL text into owned common statements.

mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

pub(crate) use self::postgres::split_postgres_statements;

use crate::StringSpan;
use crate::ast::{Dialect, Statement};
use crate::sem::AnalysisError;

/// A failed statement conversion, statement syntax error, or terminal boundary failure.
///
/// The span identifies the attempted input, in UTF-8 byte offsets. It need not
/// identify an individual token: parsers differ in their diagnostic precision.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ParseStatementError {
    message: String,
    span: StringSpan,
    kind: FailureKind,
    position: Option<StringSpan>,
}

impl ParseStatementError {
    pub(crate) fn syntax(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            kind: FailureKind::syntax(),
            position: None,
        }
    }

    pub(crate) fn boundary_discovery(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            kind: FailureKind::boundary_discovery(),
            position: None,
        }
    }

    pub(crate) fn conversion(span: StringSpan, error: &AnalysisError) -> Self {
        Self::unsupported(span, &error.to_string())
    }

    #[cfg(not(feature = "sqlite"))]
    fn unavailable_dialect(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            kind: FailureKind::unavailable_dialect(),
            position: None,
        }
    }

    pub(crate) fn unsupported(span: StringSpan, message: &str) -> Self {
        Self {
            message: message.to_owned(),
            span,
            kind: FailureKind::conversion(),
            position: None,
        }
    }

    #[cfg(feature = "sqlite")]
    pub(crate) const fn with_position(mut self, position: StringSpan) -> Self {
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

    /// Whether this is a statement syntax or boundary-discovery failure.
    ///
    /// This classification does not imply termination; inspect
    /// [`Self::is_recoverable`] to determine whether iteration can continue.
    pub const fn is_syntax(&self) -> bool {
        self.kind.is_syntax()
    }

    /// Whether no trustworthy boundary could be established for further parsing.
    ///
    /// This error is yielded once and ends iteration. Its span covers unresolved
    /// input, which may include more than one apparent statement.
    pub const fn is_boundary_discovery(&self) -> bool {
        self.kind.is_boundary_discovery()
    }

    /// Whether the iterator can continue after this failure.
    ///
    /// Selecting an unavailable dialect is also terminal, but is not a syntax
    /// or boundary-discovery error.
    ///
    /// A true result means the failed range has a known boundary; it does not
    /// promise that another statement follows it.
    pub const fn is_recoverable(&self) -> bool {
        self.kind.is_recoverable()
    }

    /// Diagnostic supplied by the parser or common AST converter.
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug)]
enum FailureKind {
    Syntax,
    Conversion,
    BoundaryDiscovery,
    #[cfg(not(feature = "sqlite"))]
    UnavailableDialect,
}

impl FailureKind {
    const fn syntax() -> Self {
        Self::Syntax
    }

    const fn conversion() -> Self {
        Self::Conversion
    }

    const fn boundary_discovery() -> Self {
        Self::BoundaryDiscovery
    }

    #[cfg(not(feature = "sqlite"))]
    const fn unavailable_dialect() -> Self {
        Self::UnavailableDialect
    }

    const fn is_recoverable(&self) -> bool {
        matches!(self, Self::Syntax | Self::Conversion)
    }

    const fn is_syntax(&self) -> bool {
        matches!(self, Self::Syntax | Self::BoundaryDiscovery)
    }

    const fn is_boundary_discovery(&self) -> bool {
        matches!(self, Self::BoundaryDiscovery)
    }
}

/// Parse SQL in the explicitly selected dialect, yielding each statement's result.
///
/// Names remain unresolved and no catalog, configuration, or files are read.
/// Conversion errors and PostgreSQL syntax errors with known statement boundaries
/// allow iteration to continue. Unrecoverable boundary-discovery errors are yielded
/// once and end iteration. Exhaust the iterator to check the entire input; an
/// empty input yields no statements.
///
/// PostgreSQL uses parser-validated boundaries, with scanner candidates and limited
/// structural recovery for malformed input. A lexical failure can yield one error
/// covering the entire input when no boundaries have been established. SQLite
/// discovers boundaries incrementally; its syntax errors end iteration.
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
                Box::new(std::iter::once(Err(
                    ParseStatementError::unavailable_dialect(
                        StringSpan::new(0, sql.len()),
                        "SQLite support requires the sqlite feature",
                    ),
                )))
            }
        };
    statements
}
