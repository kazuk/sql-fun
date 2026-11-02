use super::super::test_helpers::column_scope_query;
use crate::ast::Dialect;
use crate::sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr};

#[rstest::rstest]
#[case::original("SELECT (SELECT id FROM missing.items) FROM outer_items;")]
#[case::competing_known("SELECT (SELECT id FROM known_items, missing.items) FROM outer_items")]
#[case::unknown_first("SELECT (SELECT id FROM missing.items, known_items) FROM outer_items")]
#[case::multiple_unknown(
    "SELECT (SELECT id FROM missing.items a, missing.other b) FROM outer_items"
)]
#[case::join("SELECT (SELECT id FROM known_items CROSS JOIN missing.items) FROM outer_items")]
#[case::join_unknown_first(
    "SELECT (SELECT id FROM missing.items CROSS JOIN known_items) FROM outer_items"
)]
#[case::nested_unknown("SELECT (SELECT (SELECT id) FROM missing.items) FROM outer_items")]
#[case::deep_unknown(
    "SELECT (SELECT (SELECT id FROM missing.items) FROM local_items) FROM outer_items"
)]
#[case::shadowed_alias(
    "SELECT (SELECT outer_items.id FROM missing.items outer_items) FROM outer_items"
)]
#[case::qualified_missing("SELECT (SELECT items.id FROM missing.items) FROM outer_items")]
fn unresolved_local_relation_blocks_outer_binding(#[case] sql: &str) {
    let (mut query, context) = column_scope_query(sql, Dialect::postgres());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert!(bindings.tables.is_empty(), "{bindings:?}");
    assert_eq!(bindings.unresolved, 1, "{bindings:?}");
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "relation_not_found")
    );
}

#[rstest::rstest]
#[case::missing("SELECT (SELECT id FROM missing()) FROM outer_items")]
#[case::competing_known("SELECT (SELECT id FROM known_items, missing()) FROM outer_items")]
#[case::unknown_first("SELECT (SELECT id FROM missing(), known_items) FROM outer_items")]
#[case::join("SELECT (SELECT id FROM known_items CROSS JOIN missing()) FROM outer_items")]
#[case::nested("SELECT (SELECT (SELECT id) FROM missing()) FROM outer_items")]
fn unresolved_table_function_blocks_outer_binding(#[case] sql: &str) {
    let (mut query, context) = column_scope_query(sql, Dialect::postgres());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert!(bindings.tables.is_empty(), "{bindings:?}");
    assert_eq!(bindings.unresolved, 1, "{bindings:?}");
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "table_function_not_found")
    );
}

#[rstest::rstest]
#[case::resolved_absence("SELECT (SELECT id FROM local_items) FROM outer_items", "outer_items")]
#[case::empty_from("SELECT (SELECT id) FROM outer_items", "outer_items")]
#[case::nested_absence(
    "SELECT (SELECT (SELECT id FROM local_items)) FROM outer_items",
    "outer_items"
)]
#[case::local_binding("SELECT (SELECT id FROM known_items) FROM outer_items", "known_items")]
#[case::local_shadowing(
    "SELECT (SELECT outer_items.id FROM known_items outer_items) FROM outer_items",
    "known_items"
)]
fn conclusive_lookup_preserves_binding(#[case] sql: &str, #[case] relation: &str) {
    let (mut query, context) = column_scope_query(sql, Dialect::postgres());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert_eq!(bindings.tables, [relation]);
    assert_eq!(bindings.unresolved, 0);
    assert_eq!(context.reported_problem_count(), 0);
}

#[test]
fn explicit_outer_qualification_bypasses_unrelated_unknown_source() {
    let (mut query, context) = column_scope_query(
        "SELECT (SELECT outer_items.id FROM missing.items) FROM outer_items",
        Dialect::postgres(),
    );
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert_eq!(bindings.tables, ["outer_items"]);
    assert_eq!(bindings.unresolved, 0);
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "relation_not_found")
    );
}

#[rstest::rstest]
#[case::known("known_items a, known_items b")]
#[case::unknown_first("missing.items, known_items a, known_items b")]
#[case::unknown_last("known_items a, known_items b, missing.items")]
#[case::join("known_items a CROSS JOIN known_items b CROSS JOIN missing.items")]
fn established_ambiguity_survives_unknown_sources(#[case] from: &str) {
    let sql = format!("SELECT (SELECT id FROM {from}) FROM outer_items");
    let (mut query, context) = column_scope_query(&sql, Dialect::postgres());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert!(bindings.tables.is_empty());
    assert_eq!(bindings.ambiguous, 1);
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "ambiguous_column")
    );
}

#[test]
fn conclusive_missing_column_remains_unresolved() {
    let (mut query, context) =
        column_scope_query("SELECT absent FROM local_items", Dialect::postgres());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert!(bindings.tables.is_empty());
    assert_eq!(bindings.unresolved, 1);
    assert_eq!(context.reported_problem_count(), 1);
}

#[test]
fn unknown_input_does_not_bind_grouping_output_alias() {
    let (mut query, _) = column_scope_query(
        "SELECT 1 AS id FROM missing.items GROUP BY id",
        Dialect::postgres(),
    );
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert_eq!(bindings.unresolved, 1);
    assert_eq!(bindings.outputs, 0);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::group("SELECT (SELECT id FROM (missing_items) AS g) FROM outer_items")]
#[case::group_join(
    "SELECT (SELECT id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
)]
#[case::qualified_group(
    "SELECT (SELECT g.id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
)]
fn sqlite_group_preserves_inconclusive_lookup(#[case] sql: &str) {
    let (mut query, context) = column_scope_query(sql, Dialect::sqlite());
    let mut bindings = Bindings::default();
    query.walk(&mut bindings).unwrap();
    assert!(bindings.tables.is_empty());
    assert_eq!(bindings.unresolved, 1);
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "relation_not_found")
    );
}

#[derive(Default, Debug)]
struct Bindings {
    tables: Vec<String>,
    unresolved: usize,
    ambiguous: usize,
    outputs: usize,
}

impl crate::sem::traversal::SemanticVisitorMut for Bindings {
    fn expression(
        &mut self,
        expression: &mut SemScalarExpr,
    ) -> Result<(), crate::sem::AnalysisError> {
        if expression.output_index().is_some() {
            self.outputs += 1;
        }
        if let SemScalarExpr::ColumnRef(reference) = expression {
            if let Some(table) = reference.table_column_reference() {
                self.tables
                    .push(table.relation().local_name().as_str().to_owned());
                assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
            } else if matches!(reference, ColumnReferenceExpr::AmbiguousColumnRef(_)) {
                self.ambiguous += 1;
                assert_eq!(expression.get_type(), None);
            } else if matches!(
                reference,
                ColumnReferenceExpr::UndefinedColumn(_)
                    | ColumnReferenceExpr::UndefinedTableOrView(_)
                    | ColumnReferenceExpr::UndefinedAlias(_)
                    | ColumnReferenceExpr::UndefinedFunction(_)
            ) {
                self.unresolved += 1;
                assert_eq!(expression.get_type(), None);
            }
        }
        Ok(())
    }
}
