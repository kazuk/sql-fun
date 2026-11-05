use crate::ast::{Dialect, Statement, TargetLocationError, projection::ProjectionLocations};
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

#[cfg(test)]
mod tests {
    use crate::{
        ast::Dialect,
        sem::{SemAst, WildcardExpansion},
        test_helpers::{
            create_cache_context,
            wildcard::{catalog, expansions, statement},
        },
    };

    #[test]
    fn target_location_synchronization_disambiguates_imported_statements() {
        use crate::ast::{CteDefinition, HaveWithClause};
        let mut cache_context = create_cache_context();
        let mut owner = statement("SELECT * FROM t");
        let definition = CteDefinition::new("imported", &statement("SELECT * FROM u")).unwrap();
        owner
            .ensure_with_clause()
            .unwrap()
            .insert(0, &definition)
            .unwrap();
        let rendered = owner.deparse_sql(Dialect::postgres()).unwrap();
        owner
            .synchronize_target_locations(statement(&rendered))
            .unwrap();
        let groups = expansions(&owner);
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].location(), groups[1].location());
        assert_eq!(WildcardExpansion::rewrite(&groups, &mut owner).unwrap(), 2);
        let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(!emitted.contains('*'), "{emitted}");
        assert!(
            SemAst::from_ast(&statement(&emitted), catalog(), &mut cache_context)
                .unwrap()
                .context()
                .reported_problems()
                .is_empty()
        );
    }

    #[test]
    fn target_location_synchronization_rejects_a_different_statement_atomically() {
        let mut owner = statement("SELECT * FROM t");
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        let locations: Vec<_> = expansions(&owner)
            .iter()
            .map(|group| group.location())
            .collect();
        assert!(
            owner
                .synchronize_target_locations(statement("SELECT * FROM u"))
                .unwrap_err()
                .is_invalid_expression()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
        assert_eq!(
            expansions(&owner)
                .iter()
                .map(|group| group.location())
                .collect::<Vec<_>>(),
            locations
        );
        owner
            .synchronize_target_locations(statement(&before))
            .unwrap();
        let groups = expansions(&owner);
        assert_eq!(WildcardExpansion::rewrite(&groups, &mut owner).unwrap(), 1);
    }

    #[test]
    fn target_location_synchronization_rejects_colliding_source_locations_atomically() {
        use crate::ast::{CteDefinition, HaveWithClause};
        let mut source = statement("SELECT * FROM t");
        let definition = CteDefinition::new("imported", &statement("SELECT * FROM u")).unwrap();
        source
            .ensure_with_clause()
            .unwrap()
            .insert(0, &definition)
            .unwrap();
        let rendered = source.deparse_sql(Dialect::postgres()).unwrap();
        let mut owner = statement(&rendered);
        let before: Vec<_> = expansions(&owner)
            .iter()
            .map(|group| group.location())
            .collect();
        assert!(
            owner
                .synchronize_target_locations(source)
                .unwrap_err()
                .is_ambiguous()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), rendered);
        assert_eq!(
            expansions(&owner)
                .iter()
                .map(|group| group.location())
                .collect::<Vec<_>>(),
            before
        );
        owner
            .synchronize_target_locations(statement(&rendered))
            .unwrap();
        let groups = expansions(&owner);
        assert_eq!(WildcardExpansion::rewrite(&groups, &mut owner).unwrap(), 2);
    }
}
