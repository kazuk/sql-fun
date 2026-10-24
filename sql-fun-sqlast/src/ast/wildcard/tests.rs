mod test_helpers;

use self::test_helpers::{catalog, expansions, rewritten, statement};
use crate::ast::Dialect;
use crate::sem::ParseContext;

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
    let original = statement(sql).analyze(catalog()).unwrap();
    let (rewritten, count) = rewritten(sql);
    assert_eq!(count, expected);
    let emitted = rewritten.deparse_sql(Dialect::postgres()).unwrap();
    let reparsed = statement(&emitted).analyze(catalog()).unwrap();
    assert!(
        reparsed.context().reported_problems().is_empty(),
        "{emitted}: {:?}",
        reparsed.context().reported_problems()
    );
    let before = original.ast().result_columns().unwrap();
    let after = reparsed.ast().result_columns().unwrap();
    assert_eq!(before.len(), after.len());
    for ordinal in 0..before.len() {
        let left = before.get_column_at(ordinal).unwrap();
        let right = after.get_column_at(ordinal).unwrap();
        assert_eq!(left.column_name(), right.column_name(), "{emitted}");
        assert_eq!(left.col_type(), right.col_type(), "{emitted}");
    }
}

#[rstest::rstest]
#[case::using("SELECT * FROM t FULL JOIN u USING (id)", "COALESCE")]
#[case::natural("SELECT * FROM t NATURAL FULL JOIN u", "COALESCE")]
#[case::alias("SELECT joined.* FROM (t FULL JOIN u USING (id)) joined", "joined")]
#[case::duplicate("SELECT t.*, t.* FROM t", "AS")]
fn resolved_join_expressions_remain_executable(#[case] sql: &str, #[case] expected: &str) {
    let (rewritten, _) = rewritten(sql);
    let emitted = rewritten.deparse_sql(Dialect::postgres()).unwrap();
    assert!(
        emitted.to_uppercase().contains(&expected.to_uppercase()),
        "{emitted}"
    );
    let pair = statement(&emitted).analyze(catalog()).unwrap();
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
    assert!(owner.rewrite_wildcards(&groups).unwrap_err().is_not_found());
    assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
    let own_groups = expansions(&owner);
    assert_eq!(owner.rewrite_wildcards(&own_groups).unwrap(), 1);
}

#[test]
fn duplicate_group_rejection_preserves_owner() {
    let mut owner = statement("SELECT * FROM t");
    let mut groups = expansions(&owner);
    groups.push(groups[0].clone());
    let before = owner.deparse_sql(Dialect::postgres()).unwrap();
    assert!(
        owner
            .rewrite_wildcards(&groups)
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
        owner
            .rewrite_wildcards(&groups)
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
    assert!(owner.rewrite_wildcards(&groups).unwrap_err().is_ambiguous());
    assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
}

#[test]
fn missing_parser_location_is_rejected_without_edit() {
    use crate::sem::{ResultColumnCollection, WildcardExpansion};
    let mut owner = statement("SELECT * FROM t");
    let group = WildcardExpansion::new(None, &ResultColumnCollection::default(), 0, true, &[]);
    let before = owner.deparse_sql(Dialect::postgres()).unwrap();
    assert!(
        owner
            .rewrite_wildcards(&[group])
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
        owner
            .rewrite_wildcards(&groups)
            .unwrap_err()
            .is_empty_returning()
    );
    assert_eq!(owner.deparse_sql(Dialect::postgres()).unwrap(), before);
}

#[rstest::rstest]
#[case::new("DELETE FROM t RETURNING new.*", "new")]
#[case::old("DELETE FROM t RETURNING old.*", "old")]
fn returning_row_image_qualification_is_preserved(#[case] sql: &str, #[case] qualifier: &str) {
    let metadata = serde_json::from_value(serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": "18", "search_path": ["public"]
    }))
    .unwrap();
    let context = || catalog().with_metadata(&metadata);
    let mut owner = statement(sql);
    let analyzed = owner.analyze(context()).unwrap();
    assert!(analyzed.context().reported_problems().is_empty());
    owner
        .rewrite_wildcards(analyzed.context().analysis_state().wildcard_expansions())
        .unwrap();
    let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
    assert!(
        emitted.contains(&format!("\"{qualifier}\".\"id\"")),
        "{emitted}"
    );
    let pair = statement(&emitted).analyze(context()).unwrap();
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
    let mut owner = statement(sql);
    let original = owner.analyze(test_helpers::rows_catalog()).unwrap();
    assert!(
        original.context().reported_problems().is_empty(),
        "{:?}",
        original.context().reported_problems()
    );
    owner
        .rewrite_wildcards(original.context().analysis_state().wildcard_expansions())
        .unwrap();
    let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
    let rewritten = statement(&emitted)
        .analyze(test_helpers::rows_catalog())
        .unwrap();
    assert!(
        rewritten.context().reported_problems().is_empty(),
        "{emitted}: {:?}",
        rewritten.context().reported_problems()
    );
    let before = original.ast().result_columns().unwrap();
    let after = rewritten.ast().result_columns().unwrap();
    assert_eq!(before.len(), after.len(), "{emitted}");
    for ordinal in 0..before.len() {
        let left = before.get_column_at(ordinal).unwrap();
        let right = after.get_column_at(ordinal).unwrap();
        assert_eq!(left.column_name(), right.column_name(), "{emitted}");
        assert_eq!(left.col_type(), right.col_type(), "{emitted}");
    }
}

#[test]
fn target_location_synchronization_disambiguates_imported_statements() {
    use crate::ast::{CteDefinition, HaveWithClause};
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
    assert_eq!(owner.rewrite_wildcards(&groups).unwrap(), 2);
    let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
    assert!(!emitted.contains('*'), "{emitted}");
    assert!(
        statement(&emitted)
            .analyze(catalog())
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
    assert_eq!(owner.rewrite_wildcards(&groups).unwrap(), 1);
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
    assert_eq!(owner.rewrite_wildcards(&groups).unwrap(), 2);
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
    let metadata = serde_json::from_value(serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))
    .unwrap();
    let context = || test_helpers::rows_catalog().with_metadata(&metadata);
    let mut owner = statement(sql);
    let original = owner.analyze(context()).unwrap();
    assert!(
        original.context().reported_problems().is_empty(),
        "{:?}",
        original.context().reported_problems()
    );
    assert_eq!(
        owner
            .rewrite_wildcards(original.context().analysis_state().wildcard_expansions())
            .unwrap(),
        1
    );
    let emitted = owner.deparse_sql(Dialect::postgres()).unwrap();
    let rewritten = statement(&emitted).analyze(context()).unwrap();
    assert!(
        rewritten.context().reported_problems().is_empty(),
        "{emitted}: {:?}",
        rewritten.context().reported_problems()
    );
    let before = original.ast().result_columns().unwrap();
    let after = rewritten.ast().result_columns().unwrap();
    assert_eq!(before.len(), after.len(), "{emitted}");
    for ordinal in 0..before.len() {
        let left = before.get_column_at(ordinal).unwrap();
        let right = after.get_column_at(ordinal).unwrap();
        assert_eq!(left.column_name(), right.column_name(), "{emitted}");
        assert_eq!(left.col_type(), right.col_type(), "{emitted}");
    }
}
