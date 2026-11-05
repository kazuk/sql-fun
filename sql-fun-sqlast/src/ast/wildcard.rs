//! Location synchronization for syntax-only projection edits.
use crate::ast::{
    Dialect, FromGroup, FromItem, Join, Statement,
    projection::{ProjectionError, ProjectionLocations},
};

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
    fn invalid(message: &str) -> Self {
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

impl Statement {
    /// Copy target locations from a parser round trip of this statement.
    ///
    /// The source must render identically in the same PostgreSQL dialect and
    /// retain parser locations for every target. Only target locations change;
    /// expressions and diagnostic anchors remain attached to their original input.
    /// The consumed source is temporary parser output. All checks precede edits.
    ///
    /// # Errors
    /// Returns an error without changing this statement if rendering, statement
    /// correspondence, or parser target locations cannot be validated.
    pub fn synchronize_target_locations(
        &mut self,
        mut reparsed: Self,
    ) -> Result<(), TargetLocationError> {
        if self.dialect != Dialect::postgres() || self.dialect != reparsed.dialect {
            return Err(TargetLocationError::invalid(
                "target location synchronization requires the same PostgreSQL dialect",
            ));
        }
        let original = self
            .deparse_sql(self.dialect)
            .map_err(|error| TargetLocationError::invalid(&error.to_string()))?;
        let rendered = reparsed
            .deparse_sql(reparsed.dialect)
            .map_err(|error| TargetLocationError::invalid(&error.to_string()))?;
        if original != rendered {
            return Err(TargetLocationError::invalid(
                "target location source does not render as the destination statement",
            ));
        }
        let locations = ProjectionLocations::capture(&mut reparsed.node);
        locations
            .validate_source()
            .map_err(TargetLocationError::from)?;
        locations
            .validate_destination(&mut self.node)
            .map_err(TargetLocationError::from)?;
        locations.apply(&mut self.node);
        Ok(())
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
