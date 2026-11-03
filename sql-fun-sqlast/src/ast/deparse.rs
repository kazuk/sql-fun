//! Fallible SQL emission from the owned, potentially edited syntax tree.
mod constraints;
mod dml;
mod expression;
mod lexical;
mod query;
#[cfg(test)]
mod simple_tests;
mod statement;
mod statement_simple;
#[cfg(test)]
mod tests;

use crate::ast::{Dialect, Statement};

/// A rejected SQL rendering request.
///
/// Classification and node context are available without parsing display text.
#[derive(Debug, thiserror::Error)]
#[error("{dialect} SQL rendering at {node}: {message}")]
pub struct DeparseError {
    dialect: Dialect,
    node: String,
    message: String,
    kind: FailureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    DialectMismatch,
    UnavailableDialect,
    InvalidSyntax,
    Unsupported,
}

impl DeparseError {
    fn new(dialect: Dialect, node: &str, message: &str, kind: FailureKind) -> Self {
        Self {
            dialect,
            node: node.to_owned(),
            message: message.to_owned(),
            kind,
        }
    }
    /// Requested output dialect.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }
    /// Node or clause associated with the failure.
    pub fn node(&self) -> &str {
        &self.node
    }
    /// Actionable explanation of the failure.
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Whether the request selects a different dialect from the parsed statement.
    pub const fn is_dialect_mismatch(&self) -> bool {
        matches!(self.kind, FailureKind::DialectMismatch)
    }
    /// Whether the selected dialect requires an unavailable Cargo feature.
    pub const fn is_unavailable_dialect(&self) -> bool {
        matches!(self.kind, FailureKind::UnavailableDialect)
    }
    /// Whether retained syntax is invalid in the selected dialect or position.
    pub const fn is_invalid_syntax(&self) -> bool {
        matches!(self.kind, FailureKind::InvalidSyntax)
    }
    /// Whether a represented node has no rendering implementation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(self.kind, FailureKind::Unsupported)
    }
    pub(super) fn context(mut self, node: &str) -> Self {
        self.node = format!("{node}/{}", self.node);
        self
    }
}

pub(in crate::ast) trait SqlEmit {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError>;
}

impl<T: SqlEmit> SqlEmit for Box<T> {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.as_ref().emit(writer)
    }
}

pub(in crate::ast) struct SqlWriter {
    sql: String,
    dialect: Dialect,
}

impl SqlWriter {
    pub(in crate::ast) fn into_sql(self) -> String {
        self.sql
    }
    pub(in crate::ast) const fn new(dialect: Dialect) -> Self {
        Self {
            sql: String::new(),
            dialect,
        }
    }
    pub(in crate::ast) const fn dialect(&self) -> Dialect {
        self.dialect
    }
    pub(in crate::ast) const fn is_postgres(&self) -> bool {
        matches!(self.dialect, Dialect::Postgres)
    }
    pub(in crate::ast) fn sql(&mut self, fragment: &str) {
        self.sql.push_str(fragment);
    }
    pub(in crate::ast) fn number(&mut self, value: impl std::fmt::Display) {
        self.sql.push_str(&value.to_string());
    }
    pub(in crate::ast) fn ident(&mut self, name: &str) -> Result<(), DeparseError> {
        if name.contains('\0') || (self.is_postgres() && name.is_empty()) {
            return self.invalid("identifier", "identifier is empty or contains NUL");
        }
        self.sql(&crate::identifier::QuotedIdentifier::from(name).to_string());
        Ok(())
    }
    pub(in crate::ast) fn string(&mut self, value: &str) -> Result<(), DeparseError> {
        if value.contains('\0') {
            return self.invalid("literal", "SQL text literals cannot contain NUL");
        }
        if self.is_postgres() && value.contains('\\') {
            self.sql("E'");
            self.sql(&value.replace('\\', "\\\\").replace('\'', "''"));
        } else {
            self.sql("'");
            self.sql(&value.replace('\'', "''"));
        }
        self.sql("'");
        Ok(())
    }
    pub(in crate::ast) fn list<T: SqlEmit>(
        &mut self,
        values: &[T],
        separator: &str,
    ) -> Result<(), DeparseError> {
        for (index, value) in values.iter().enumerate() {
            if index != 0 {
                self.sql(separator);
            }
            value.emit(self)?;
        }
        Ok(())
    }
    pub(in crate::ast) fn names(
        &mut self,
        values: &[String],
        separator: &str,
    ) -> Result<(), DeparseError> {
        for (index, value) in values.iter().enumerate() {
            if index != 0 {
                self.sql(separator);
            }
            self.ident(value)?;
        }
        Ok(())
    }
    pub(in crate::ast) fn postgres_only(&self, node: &str) -> Result<(), DeparseError> {
        if self.is_postgres() {
            Ok(())
        } else {
            self.invalid(node, "construct is not legal SQLite SQL")
        }
    }
    pub(in crate::ast) fn invalid<T>(&self, node: &str, message: &str) -> Result<T, DeparseError> {
        Err(DeparseError::new(
            self.dialect,
            node,
            message,
            FailureKind::InvalidSyntax,
        ))
    }
    pub(in crate::ast) fn invalid_error(&self, node: &str, message: &str) -> DeparseError {
        DeparseError::new(self.dialect, node, message, FailureKind::InvalidSyntax)
    }
    pub(in crate::ast) fn unsupported<T>(&self, node: &str) -> Result<T, DeparseError> {
        Err(DeparseError::new(
            self.dialect,
            node,
            "represented syntax has no rendering implementation",
            FailureKind::Unsupported,
        ))
    }
}

impl Statement {
    /// Render the complete current syntax tree as one deterministic SQL statement.
    ///
    /// The output has no trailing semicolon. Ordinary comments and formatting are
    /// not retained; SQLite type trivia that affects declared types is preserved. The dialect must match the explicit parsing choice; this is
    /// not a dialect translation operation. Numbered parameter indices are never
    /// reassigned, including repeated indices and gaps. SQLite named and anonymous
    /// parameters use their stored numbered slots; bind the output by those slots.
    /// No schema is inspected.
    ///
    /// # Errors
    /// Returns [`DeparseError`] for a dialect mismatch, unavailable feature,
    /// invalid syntax/placement, or an unsupported represented node. The statement
    /// remains unchanged and partial output is never returned.
    pub fn deparse_sql(&self, dialect: Dialect) -> Result<String, DeparseError> {
        if dialect != self.dialect {
            return Err(DeparseError::new(
                dialect,
                "statement",
                "output dialect differs from the parsed statement",
                FailureKind::DialectMismatch,
            ));
        }
        #[cfg(not(feature = "sqlite"))]
        if dialect == Dialect::sqlite() {
            return Err(DeparseError::new(
                dialect,
                "statement",
                "enable the sqlite Cargo feature",
                FailureKind::UnavailableDialect,
            ));
        }
        let mut writer = SqlWriter::new(dialect);
        self.node.emit(&mut writer)?;
        Ok(writer.sql)
    }
}

/// Render a semantic name using the same checked identifier emitter as the AST.
pub(crate) fn render_full_name(
    name: &crate::sem::FullName,
    dialect: Dialect,
) -> Result<String, DeparseError> {
    let mut writer = SqlWriter::new(dialect);
    if let Some(catalog) = name.catalog() {
        writer.postgres_only("catalog-qualified name")?;
        writer.ident(catalog.as_str())?;
        writer.sql(".");
    }
    if let Some(schema) = name.schema() {
        writer.ident(schema.as_str())?;
        writer.sql(".");
    }
    writer.ident(name.name().as_str())?;
    Ok(writer.into_sql())
}
