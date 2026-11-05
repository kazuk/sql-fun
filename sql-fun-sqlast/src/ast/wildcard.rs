//! Location synchronization for syntax-only projection edits.
use crate::ast::{FromGroup, FromItem, Join, projection::ProjectionError};

/// A rejected synchronization of syntax projection locations.
/// The destination statement remains unchanged on failure.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TargetLocationError(TargetLocationProblem);

#[derive(Debug, thiserror::Error)]
enum TargetLocationProblem {
    #[error("wildcard location was not found")]
    NotFound,
    #[error("wildcard location is ambiguous")]
    Ambiguous,
    #[error("{0}")]
    InvalidExpression(String),
}

impl TargetLocationError {
    pub(crate) fn invalid(message: &str) -> Self {
        Self(TargetLocationProblem::invalid(message))
    }
    /// Whether the parser output has a target without a lexical location.
    pub const fn is_not_found(&self) -> bool {
        self.0.is_not_found()
    }
    /// Whether multiple parser targets share a lexical location.
    pub const fn is_ambiguous(&self) -> bool {
        self.0.is_ambiguous()
    }
    /// Whether the statements cannot be rendered or matched for synchronization.
    pub const fn is_invalid_expression(&self) -> bool {
        self.0.is_invalid_expression()
    }
}

impl TargetLocationProblem {
    const fn not_found() -> Self {
        Self::NotFound
    }
    const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    fn invalid(message: &str) -> Self {
        Self::InvalidExpression(message.to_owned())
    }
    const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    const fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }
    const fn is_invalid_expression(&self) -> bool {
        matches!(self, Self::InvalidExpression(_))
    }
}

impl From<ProjectionError> for TargetLocationError {
    fn from(error: ProjectionError) -> Self {
        if error.is_missing() {
            Self(TargetLocationProblem::not_found())
        } else if error.is_ambiguous() {
            Self(TargetLocationProblem::ambiguous())
        } else {
            Self::invalid("target collections differ after the parser round trip")
        }
    }
}

impl FromItem {
    pub(crate) fn clear_projection_alias(&mut self) {
        match self {
            Self::Join(node) => node.clear_projection_alias(),
            Self::Group(node) => node.clear_projection_alias(),
            _ => {}
        }
    }
}
impl Join {
    fn clear_projection_alias(&mut self) {
        self.alias = None;
    }
}
impl FromGroup {
    fn clear_projection_alias(&mut self) {
        self.alias = None;
    }
}
