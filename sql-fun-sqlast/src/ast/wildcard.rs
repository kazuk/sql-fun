//! Location synchronization for syntax-only projection edits.
use crate::{
    ast::{Dialect, Statement, projection::ProjectionLocations},
    wildcard_rewrite_error::WildcardRewriteError,
};
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
    ) -> Result<(), WildcardRewriteError> {
        if self.dialect != Dialect::postgres() || self.dialect != reparsed.dialect {
            return Err(WildcardRewriteError::invalid(
                "target location synchronization requires the same PostgreSQL dialect",
            ));
        }
        let original = self
            .deparse_sql(self.dialect)
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        let rendered = reparsed
            .deparse_sql(reparsed.dialect)
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        if original != rendered {
            return Err(WildcardRewriteError::invalid(
                "target location source does not render as the destination statement",
            ));
        }
        let locations = ProjectionLocations::capture(&mut reparsed.node);
        locations
            .validate_source()
            .map_err(WildcardRewriteError::from)?;
        locations
            .validate_destination(&mut self.node)
            .map_err(WildcardRewriteError::from)?;
        locations.apply(&mut self.node);
        Ok(())
    }
}
