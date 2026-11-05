//! Errors for checked semantic wildcard projection edits.
use crate::ast::projection::ProjectionError;
/// A rejected wildcard rewrite. The statement remains unchanged on failure.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct WildcardRewriteError(RewriteProblem);

#[derive(Debug, thiserror::Error)]
enum RewriteProblem {
    #[error("wildcard expansion has no lexical location")]
    MissingLocation,
    #[error("wildcard location was not found")]
    NotFound,
    #[error("wildcard location is ambiguous")]
    Ambiguous,
    #[error("duplicate wildcard expansion location")]
    Duplicate,
    #[error("lexical location does not identify a wildcard projection")]
    WrongKind,
    #[error("wildcard replacement would remove the complete RETURNING projection")]
    EmptyReturning,
    #[error("{0}")]
    InvalidExpression(String),
}

impl RewriteProblem {
    pub(crate) const fn missing_location() -> Self {
        Self::MissingLocation
    }
    pub(crate) const fn is_missing_location(&self) -> bool {
        matches!(self, Self::MissingLocation)
    }
    pub(crate) const fn not_found() -> Self {
        Self::NotFound
    }
    pub(crate) const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    pub(crate) const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    pub(crate) const fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }
    pub(crate) const fn duplicate_location() -> Self {
        Self::Duplicate
    }
    pub(crate) const fn is_duplicate_location(&self) -> bool {
        matches!(self, Self::Duplicate)
    }
    pub(crate) const fn wrong_node_kind() -> Self {
        Self::WrongKind
    }
    pub(crate) const fn is_wrong_node_kind(&self) -> bool {
        matches!(self, Self::WrongKind)
    }
    pub(crate) const fn empty_returning() -> Self {
        Self::EmptyReturning
    }
    pub(crate) const fn is_empty_returning(&self) -> bool {
        matches!(self, Self::EmptyReturning)
    }
    pub(crate) fn invalid(message: &str) -> Self {
        Self::InvalidExpression(message.to_owned())
    }
    pub(crate) const fn is_invalid_expression(&self) -> bool {
        matches!(self, Self::InvalidExpression(_))
    }
}

impl WildcardRewriteError {
    pub(crate) fn invalid(message: &str) -> Self {
        Self(RewriteProblem::invalid(message))
    }
    pub(crate) const fn missing_location() -> Self {
        Self(RewriteProblem::missing_location())
    }
    /// Whether an expansion has no parser-provided location.
    pub const fn is_missing_location(&self) -> bool {
        self.0.is_missing_location()
    }
    pub(crate) const fn not_found() -> Self {
        Self(RewriteProblem::not_found())
    }
    /// Whether no projection exists at the supplied location.
    pub const fn is_not_found(&self) -> bool {
        self.0.is_not_found()
    }
    pub(crate) const fn ambiguous() -> Self {
        Self(RewriteProblem::ambiguous())
    }
    /// Whether multiple projection targets share the supplied location.
    pub const fn is_ambiguous(&self) -> bool {
        self.0.is_ambiguous()
    }
    pub(crate) const fn duplicate_location() -> Self {
        Self(RewriteProblem::duplicate_location())
    }
    /// Whether two expansion groups request the same location.
    pub const fn is_duplicate_location(&self) -> bool {
        self.0.is_duplicate_location()
    }
    pub(crate) const fn wrong_node_kind() -> Self {
        Self(RewriteProblem::wrong_node_kind())
    }
    /// Whether the location identifies a projection other than a wildcard.
    pub const fn is_wrong_node_kind(&self) -> bool {
        self.0.is_wrong_node_kind()
    }
    pub(crate) const fn empty_returning() -> Self {
        Self(RewriteProblem::empty_returning())
    }
    /// Whether a replacement would erase the complete RETURNING projection.
    pub const fn is_empty_returning(&self) -> bool {
        self.0.is_empty_returning()
    }
    /// Whether a resolved output cannot be emitted as an explicit projection.
    pub const fn is_invalid_expression(&self) -> bool {
        self.0.is_invalid_expression()
    }
}

impl From<ProjectionError> for WildcardRewriteError {
    fn from(error: ProjectionError) -> Self {
        if error.is_missing() {
            Self::not_found()
        } else if error.is_ambiguous() {
            Self::ambiguous()
        } else if error.is_duplicate() {
            Self::duplicate_location()
        } else if error.is_wrong_kind() {
            Self::wrong_node_kind()
        } else if error.is_empty_returning() {
            Self::empty_returning()
        } else {
            Self::invalid("target collections differ after the parser round trip")
        }
    }
}
