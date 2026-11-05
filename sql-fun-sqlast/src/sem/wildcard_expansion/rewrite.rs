//! Location-checked wildcard projection replacement in the owning common AST.

mod error;
mod rows_source;
mod source;

pub use self::error::WildcardRewriteError;
use self::source::{SourcePlans, ordinal_reference, typed_record_reference};
use crate::{
    ast::{
        Expr, Name, Statement, Target,
        projection::{ProjectionReplacements, ProjectionScopes},
    },
    sem::WildcardExpansion,
};

impl WildcardExpansion {
    /// Replace table-dependent wildcard projections using one semantic analysis.
    /// Returns the number of replaced projection items.
    ///
    /// Expansions must originate from this same, unmodified statement. Lexical
    /// locations are relative to the PostgreSQL parser input. All lookups
    /// and replacement expressions are checked before any edit is committed;
    /// an error leaves the complete statement unchanged and allows retry.
    /// Unrelated wildcard expansions are ignored. An empty expansion removes its
    /// projection item, including the final item in a zero-column SELECT.
    ///
    /// # Errors
    /// Rejects missing/ambiguous locations, non-wildcard destinations, and
    /// unresolved expressions that cannot represent an executable projection.
    pub fn rewrite(
        expansions: &[Self],
        statement: &mut Statement,
    ) -> Result<usize, WildcardRewriteError> {
        let mut staged = statement.clone();
        let mut plan = WildcardRewritePlan::new(expansions);
        plan.prepare(&mut staged)?;
        let count = plan.apply(&mut staged)?;
        *statement = staged;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{SemAst, WildcardExpansion},
        test_helpers::{
            create_cache_context, wildcard as test_helpers,
            wildcard::{ResultShapeAssertions, catalog, expansions, rewritten, statement},
        },
    };

    #[rstest::rstest]
    #[case::repeated("SELECT t.*, *, t.* FROM t", 3)]
    #[case::nested("SELECT * FROM (SELECT * FROM t) q", 1)]
    #[case::cte("WITH q AS (SELECT * FROM t) SELECT * FROM q", 1)]
    #[case::scalar("SELECT (SELECT id FROM (SELECT * FROM t) q) FROM t", 1)]
    #[case::set("SELECT * FROM t UNION ALL SELECT * FROM t", 2)]
    #[case::insert("INSERT INTO t VALUES (1, 2) RETURNING *", 1)]
    #[case::update("UPDATE t SET name = 1 RETURNING *", 1)]
    #[case::delete("DELETE FROM t RETURNING *", 1)]
    #[case::zero_columns("SELECT * FROM empty", 1)]
    #[case::correlated_order("SELECT 1 FROM (t CROSS JOIN u) j ORDER BY EXISTS(SELECT j.*)", 1)]
    #[case::correlated("SELECT EXISTS(SELECT j.*) FROM (t CROSS JOIN u) j", 1)]
    #[case::correlated_set(
        "SELECT EXISTS(SELECT j.* UNION ALL SELECT j.*) FROM (t CROSS JOIN u) j",
        2
    )]
    #[case::deep_correlation("SELECT EXISTS(SELECT EXISTS(SELECT j.*)) FROM (t CROSS JOIN u) j", 1)]
    #[case::empty_outer_projection("SELECT FROM (t CROSS JOIN u) j WHERE EXISTS(SELECT j.*)", 1)]
    #[case::shadowed_alias(
        "SELECT EXISTS(SELECT j.* FROM (u CROSS JOIN t) j) FROM (t CROSS JOIN u) j",
        1
    )]
    #[case::correlated_lateral(
        "SELECT child.* FROM (t CROSS JOIN u) j CROSS JOIN LATERAL (SELECT j.*) child",
        1
    )]
    #[case::nested_source_and_target_edits(
        "SELECT k.* FROM ((SELECT j.* FROM (t CROSS JOIN u) j) child(a,b,id,d) CROSS JOIN t other) k",
        2
    )]
    #[case::duplicate_join_alias("SELECT j.* FROM (t JOIN u ON true) AS j", 1)]
    #[case::mixed_duplicate_alias("SELECT * FROM (t JOIN u ON true) AS j CROSS JOIN t AS other", 1)]
    #[case::duplicate_alias_using(
        "SELECT * FROM (t JOIN u ON true) AS j JOIN (SELECT 1 AS name) v USING (name)",
        1
    )]
    #[case::nested_duplicate_alias(
        "SELECT k.* FROM ((t JOIN u ON true) AS j CROSS JOIN t AS other) AS k",
        1
    )]
    #[case::duplicate_alias_record("SELECT j.*, record_arg(j.*) FROM (t JOIN u ON true) AS j", 1)]
    #[case::unique_outer_alias(
        "SELECT k.* FROM ((t JOIN u ON true) j CROSS JOIN empty) k(a,b,c,d)",
        1
    )]
    #[case::zero_width_returning("DELETE FROM empty RETURNING *, 1", 1)]

    fn rewrite_roundtrip_preserves_result_shape(#[case] sql: &str, #[case] expected: usize) {
        let mut cache_context = create_cache_context();
        let original = SemAst::from_ast(&statement(sql), catalog(), &mut cache_context).unwrap();
        let (rewritten, count) = rewritten(sql);
        assert_eq!(count, expected);
        let emitted = rewritten.deparse_sql(Dialect::postgres()).unwrap();
        let reparsed =
            SemAst::from_ast(&statement(&emitted), catalog(), &mut cache_context).unwrap();
        assert!(
            reparsed.context().reported_problems().is_empty(),
            "{emitted}: {:?}",
            reparsed.context().reported_problems()
        );
        let before = original.ast().result_columns().unwrap();
        let after = reparsed.ast().result_columns().unwrap();
        before.assert_same_result_shape(after, &emitted);
    }

    #[rstest::rstest]
    #[case::using("SELECT * FROM t FULL JOIN u USING (id)", "COALESCE")]
    #[case::natural("SELECT * FROM t NATURAL FULL JOIN u", "COALESCE")]
    #[case::alias("SELECT joined.* FROM (t FULL JOIN u USING (id)) joined", "joined")]
    #[case::duplicate("SELECT t.*, t.* FROM t", "AS")]
    fn resolved_join_expressions_remain_executable(#[case] sql: &str, #[case] expected: &str) {
        let mut cache_context = create_cache_context();
        let (rewritten, _) = rewritten(sql);
        let emitted = rewritten.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            emitted.to_uppercase().contains(&expected.to_uppercase()),
            "{emitted}"
        );
        let pair = SemAst::from_ast(&statement(&emitted), catalog(), &mut cache_context).unwrap();
        assert!(
            pair.context().reported_problems().is_empty(),
            "{emitted}: {:?}",
            pair.context().reported_problems()
        );
    }

    #[test]
    fn missing_location_preserves_owner_and_allows_retry() {
        let source = statement("SELECT * FROM t");
        let groups = expansions(&source);
        let mut owner = statement("(SELECT * FROM t)");
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_not_found()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
        let own_groups = expansions(&owner);
        assert_eq!(
            WildcardExpansion::rewrite(&own_groups, &mut owner).unwrap(),
            1
        );
    }

    #[test]
    fn duplicate_group_rejection_preserves_owner() {
        let mut owner = statement("SELECT * FROM t");
        let mut groups = expansions(&owner);
        groups.push(groups[0].clone());
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_duplicate_location()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    }

    #[test]
    fn wrong_node_kind_is_rejected_without_edit() {
        let groups = expansions(&statement("SELECT * FROM t"));
        let mut owner = statement("SELECT 1 FROM t");
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_wrong_node_kind()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    }

    #[test]
    fn fixed_projection_star_is_preserved() {
        let sql = "SELECT * FROM (SELECT 1 AS id) q";
        let (owner, count) = rewritten(sql);
        assert_eq!(count, 0);
        assert!(
            owner
                .deparse_sql(Dialect::postgres())
                .unwrap()
                .contains('*')
        );
    }

    #[test]
    fn ambiguous_target_locations_preserve_owner() {
        use crate::ast::{CteDefinition, HaveWithClause};
        let mut owner = statement("SELECT * FROM t");
        let groups = expansions(&owner);
        let definition = CteDefinition::new("imported", &statement("SELECT * FROM u")).unwrap();
        owner
            .ensure_with_clause()
            .unwrap()
            .insert(0, &definition)
            .unwrap();
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_ambiguous()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    }

    #[test]
    fn missing_parser_location_is_rejected_without_edit() {
        use crate::sem::{ResultColumnCollection, WildcardExpansion};
        let mut owner = statement("SELECT * FROM t");
        let group = WildcardExpansion::new(None, &ResultColumnCollection::default(), 0, true, &[]);
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&[group], &mut owner)
                .unwrap_err()
                .is_missing_location()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    }

    #[test]
    fn empty_returning_is_rejected_without_edit() {
        let mut owner = statement("DELETE FROM empty RETURNING *");
        let groups = expansions(&owner);
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_empty_returning()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    }

    #[rstest::rstest]
    #[case::new("DELETE FROM t RETURNING new.*", "new")]
    #[case::old("DELETE FROM t RETURNING old.*", "old")]
    fn returning_row_image_qualification_is_preserved(#[case] sql: &str, #[case] qualifier: &str) {
        let mut cache_context = create_cache_context();
        let metadata = serde_json::from_value(serde_json::json!({
            "engine": "postgres", "dialect": "postgres", "version": "18", "search_path": ["public"]
        }))
        .unwrap();
        let context = || catalog().with_metadata(&metadata);
        let mut owner = statement(sql);
        let analyzed = SemAst::from_ast(&owner, context(), &mut cache_context).unwrap();
        assert!(analyzed.context().reported_problems().is_empty());
        WildcardExpansion::rewrite(
            analyzed.context().analysis_state().wildcard_expansions(),
            &mut owner,
        )
        .unwrap();
        let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            emitted.contains(&format!(r#""{qualifier}"."id""#)),
            "{emitted}"
        );
        let pair = SemAst::from_ast(&statement(&emitted), context(), &mut cache_context).unwrap();
        assert!(
            pair.context().reported_problems().is_empty(),
            "{emitted}: {:?}",
            pair.context().reported_problems()
        );
    }

    #[rstest::rstest]
    #[case::explicit("SELECT r.* FROM ROWS FROM (rows_fun(), rows_fun()) r")]
    #[case::delete("DELETE FROM t USING ROWS FROM (rows_fun(), rows_fun()) r RETURNING r.*")]
    #[case::update("UPDATE t SET name=1 FROM ROWS FROM (rows_fun(), rows_fun()) r RETURNING r.*")]
    #[case::implicit("SELECT * FROM ROWS FROM (rows_fun(), rows_fun())")]
    #[case::nested_alias(
        "SELECT k.* FROM (ROWS FROM (rows_fun(),rows_fun()) r CROSS JOIN empty) k(a,b,c,d)"
    )]
    #[case::ordinality(
        "SELECT r.* FROM ROWS FROM (rows_fun(), rows_fun()) WITH ORDINALITY r(x,x,y,y,n)"
    )]
    fn rows_from_expansion_keeps_duplicate_outputs_stable(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let mut owner = statement(sql);
        let original =
            SemAst::from_ast(&owner, test_helpers::rows_catalog(), &mut cache_context).unwrap();
        assert!(
            original.context().reported_problems().is_empty(),
            "{:?}",
            original.context().reported_problems()
        );
        WildcardExpansion::rewrite(
            original.context().analysis_state().wildcard_expansions(),
            &mut owner,
        )
        .unwrap();
        let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
        let rewritten = SemAst::from_ast(
            &statement(&emitted),
            test_helpers::rows_catalog(),
            &mut cache_context,
        )
        .unwrap();
        assert!(
            rewritten.context().reported_problems().is_empty(),
            "{emitted}: {:?}",
            rewritten.context().reported_problems()
        );
        let before = original.ast().result_columns().unwrap();
        let after = rewritten.ast().result_columns().unwrap();
        before.assert_same_result_shape(after, &emitted);
    }

    #[rstest::rstest]
    #[case::duplicate_join(
        "MERGE INTO t target USING (t a CROSS JOIN u b) j ON true WHEN MATCHED THEN DELETE RETURNING j.*"
    )]
    #[case::rows(
        "MERGE INTO t target USING ROWS FROM (rows_fun(),rows_fun()) r ON true WHEN MATCHED THEN DELETE RETURNING r.*"
    )]
    fn merge_returning_fixes_its_source_projection(
        #[case] sql: &str,
        #[values("17", "18")] version: &str,
    ) {
        let mut cache_context = create_cache_context();
        let metadata = serde_json::from_value(serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))
    .unwrap();
        let context = || test_helpers::rows_catalog().with_metadata(&metadata);
        let mut owner = statement(sql);
        let original = SemAst::from_ast(&owner, context(), &mut cache_context).unwrap();
        assert!(
            original.context().reported_problems().is_empty(),
            "{:?}",
            original.context().reported_problems()
        );
        assert_eq!(
            WildcardExpansion::rewrite(
                original.context().analysis_state().wildcard_expansions(),
                &mut owner
            )
            .unwrap(),
            1
        );
        let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
        let rewritten =
            SemAst::from_ast(&statement(&emitted), context(), &mut cache_context).unwrap();
        assert!(
            rewritten.context().reported_problems().is_empty(),
            "{emitted}: {:?}",
            rewritten.context().reported_problems()
        );
        let before = original.ast().result_columns().unwrap();
        let after = rewritten.ast().result_columns().unwrap();
        before.assert_same_result_shape(after, &emitted);
    }

    #[test]
    fn a_late_invalid_target_preserves_sources_and_allows_retry() {
        let mut cache_context = create_cache_context();
        let groups = expansions(&statement("SELECT j.*, j.* FROM (t CROSS JOIN u) j"));
        let mut owner = statement("SELECT j.*, 1   FROM (t CROSS JOIN u) j");
        let before = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            WildcardExpansion::rewrite(&groups, &mut owner)
                .unwrap_err()
                .is_wrong_node_kind()
        );
        assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
        let own_groups = expansions(&owner);
        assert_eq!(
            WildcardExpansion::rewrite(&own_groups, &mut owner).unwrap(),
            1
        );
        let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
        assert!(
            SemAst::from_ast(&statement(&emitted), catalog(), &mut cache_context)
                .unwrap()
                .context()
                .reported_problems()
                .is_empty()
        );
    }
}

struct WildcardRewritePlan<'a> {
    expansions: &'a [WildcardExpansion],
    sources: SourcePlans,
    targets: ProjectionReplacements,
}
impl<'a> WildcardRewritePlan<'a> {
    const fn new(expansions: &'a [WildcardExpansion]) -> Self {
        Self {
            expansions,
            sources: SourcePlans::new(),
            targets: ProjectionReplacements::new(),
        }
    }
    fn prepare(&mut self, statement: &mut Statement) -> Result<(), WildcardRewriteError> {
        let scopes = ProjectionScopes::capture(statement);
        for expansion in self.expansions {
            if !expansion.is_table_dependent() {
                continue;
            }
            let Some(location) = expansion.location() else {
                return Err(WildcardRewriteError::missing_location());
            };
            expansion.prepare_sources(&scopes, &mut self.sources)?;
            let targets = expansion.rewrite_targets()?;
            self.targets
                .add(location, &targets)
                .map_err(WildcardRewriteError::from)?;
        }
        self.targets
            .validate_wildcards(statement)
            .map_err(WildcardRewriteError::from)
    }
    fn apply(&mut self, statement: &mut Statement) -> Result<usize, WildcardRewriteError> {
        let count = self.targets.len();
        self.sources.stage(statement)?;
        self.targets.apply(statement);
        Ok(count)
    }
}

impl WildcardExpansion {
    fn rewrite_targets(&self) -> Result<Vec<Target>, WildcardRewriteError> {
        let bindings = self.qualifiers();
        let mut targets = Vec::new();
        for (index, column) in self.columns().iter().enumerate() {
            let expression = if let Some(record) = self.typed_record_at(index) {
                typed_record_reference(record)
            } else if let Some((alias, ordinal)) = self.source_ordinal_at(index) {
                ordinal_reference(alias, ordinal)
            } else if let Some(parts) = self.reference_at(index) {
                Expr::projection_column(&Name::from_parts(parts))
            } else {
                let Some(expression) = column.expression() else {
                    return Err(WildcardRewriteError::invalid(
                        "wildcard result has no resolved expression",
                    ));
                };
                expression.projection_expression(bindings)?
            };
            let Some(alias) = column.column_name() else {
                return Err(WildcardRewriteError::invalid(
                    "wildcard result has no output name",
                ));
            };
            targets.push(Target::projection(&expression, alias.as_str()));
        }
        Ok(targets)
    }
}
