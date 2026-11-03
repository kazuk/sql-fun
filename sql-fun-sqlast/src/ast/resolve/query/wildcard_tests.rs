//! Semantic wildcard groups use the same small relation fixtures as name resolution.

#[cfg(feature = "sqlite")]
use crate::ast::resolve::query::test_helpers::resolve;
use crate::{
    ast::{Dialect, resolve::query::test_helpers::table_function_query},
    parse_statements,
    sem::{MergeRecordRole, ResultColumn, WildcardExpansion},
    test_helpers::create_cache_context,
};

use crate::{ast::resolve::query::test_helpers::source_namespace_statement, context::ParseContext};

#[rstest::rstest]
#[case::zero_width("SELECT * FROM a.zero_columns", &[7], &[true], &[""])]
#[case::qualified_zero_width("SELECT z.* FROM a.zero_columns z", &[7], &[true], &[""])]
#[case::relation_zero_width("SELECT a.zero_columns.* FROM a.zero_columns", &[7], &[true], &[""])]
#[case::unqualified("SELECT * FROM a.t", &[7], &[true], &["id,only_a"])]
#[case::qualified("SELECT t.* FROM a.t", &[7], &[true], &["id,only_a"])]
#[case::repeated("SELECT t.*, t.* FROM a.t", &[7, 12], &[true, true], &["id,only_a", "id,only_a"])]
#[case::view("SELECT * FROM a.v", &[7], &[false], &["id"])]
#[case::fixed_derived("SELECT * FROM (SELECT id FROM a.t) s", &[7], &[false], &["id"])]
#[case::nested("SELECT * FROM (SELECT * FROM a.t) s", &[22, 7], &[true, false], &["id,only_a", "id,only_a"])]
#[case::cte("WITH q AS (SELECT * FROM a.t) SELECT * FROM q", &[18, 37], &[true, false], &["id,only_a", "id,only_a"])]
#[case::using("SELECT * FROM a.t JOIN b.t USING (id)", &[7], &[true], &["id,only_a,only_b"])]
#[case::natural("SELECT * FROM a.t NATURAL JOIN b.t", &[7], &[true], &["id,only_a,only_b"])]
#[case::full("SELECT * FROM a.t FULL JOIN b.t USING (id)", &[7], &[true], &["id,only_a,only_b"])]
fn retains_groups_names_order_and_origins(
    #[case] sql: &str,
    #[case] locations: &[usize],
    #[case] dependencies: &[bool],
    #[case] names: &[&str],
) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), locations.len());
    groups.assert_groups(locations, dependencies, names);
}

#[rstest::rstest]
#[case::insert("INSERT INTO a.t(id, only_a) VALUES (1, 2) RETURNING *")]
#[case::update("UPDATE a.t SET only_a = 2 RETURNING *")]
#[case::delete("DELETE FROM a.t RETURNING *")]
fn returning_records_the_destination_expansion(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].location(), sql.find('*'));
    assert!(groups[0].is_table_dependent());
    assert_eq!(groups[0].columns().len(), 2);
}

#[test]
fn aliased_merged_projection_retains_visible_reference() {
    let analyzed = source_namespace_statement("SELECT j.* FROM (a.t FULL JOIN b.t USING (id)) j");
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].reference_at(0),
        Some(["j".to_owned(), "id".to_owned()].as_slice())
    );
}

#[test]
fn next_analysis_resets_previous_groups() {
    let mut cache_context = create_cache_context();
    let analyzed = source_namespace_statement("SELECT * FROM a.t");
    assert_eq!(
        analyzed
            .context()
            .analysis_state()
            .wildcard_expansions()
            .len(),
        1
    );
    let statement = parse_statements("SELECT 1", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let next = statement
        .analyze(analyzed.into_context(), &mut cache_context)
        .unwrap();
    assert!(
        next.context()
            .analysis_state()
            .wildcard_expansions()
            .is_empty()
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_groups_do_not_require_lexical_locations() {
    let (_, context) = resolve(
        "SELECT s.* FROM (SELECT 1 AS id) s",
        Dialect::sqlite(),
        true,
    )
    .unwrap();
    let groups = context.analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].location(), None);
    assert!(!groups[0].is_table_dependent());
    assert_eq!(
        groups[0].columns()[0]
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        "id"
    );
}

#[rstest::rstest]
#[case::fixed_record("SELECT r.* FROM out_fun() r", false)]
#[case::named_composite("SELECT r.* FROM pair_fun() r", false)]
#[case::table_row("SELECT r.* FROM row_fun() r", true)]
#[case::rows_from("SELECT r.* FROM ROWS FROM (row_fun(), pair_fun()) r", true)]
fn classifies_function_output_by_its_resolved_shape(
    #[case] sql: &str,
    #[case] table_dependent: bool,
) {
    let (_, context) = table_function_query(sql, false);
    assert!(
        context.reported_problems().is_empty(),
        "{:?}",
        context.reported_problems()
    );
    let groups = context.analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].is_table_dependent(), table_dependent);
}

#[rstest::rstest]
#[case::explicit("SELECT r.* FROM ROWS FROM (row_fun(), pair_fun()) WITH ORDINALITY r(x,x,y,n)", "r", &["x", "x", "y", "n"], Some("n"), "0:a=x;1:a=x;1:b=y;")]
#[case::implicit("SELECT row_fun.* FROM ROWS FROM (row_fun(), pair_fun())", "row_fun", &["a", "a", "b"], None, "0:a=a;1:a=a;1:b=b;")]
fn rows_sources_retain_each_function_layout_before_aliasing(
    #[case] sql: &str,
    #[case] alias: &str,
    #[case] output_names: &[&str],
    #[case] ordinality: Option<&str>,
    #[case] expected_layout: &str,
) {
    let (_, context) = table_function_query(sql, false);
    assert!(
        context.reported_problems().is_empty(),
        "{:?}",
        context.reported_problems()
    );
    let group = &context.analysis_state().wildcard_expansions()[0];
    let [source] = group.rows_sources() else {
        panic!("one resolved ROWS FROM source")
    };
    assert_eq!(source.alias(), alias);
    assert_eq!(source.function_count(), 2);
    assert!(source.has_valid_output_layout());
    let mut retained = String::new();
    source
        .try_project_functions(|index, projection| {
            projection.project_columns(|physical, visible| {
                retained.push_str(&format!("{index}:{physical}={visible};"))
            });
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap();
    assert_eq!(retained, expected_layout);
    assert_eq!(source.ordinality_name(), ordinality);
    for index in 0..output_names.len() {
        assert_eq!(group.source_ordinal_at(index), Some((alias, index)));
    }
}

#[rstest::rstest]
#[case::direct("SELECT (pair_fun()).*")]
#[case::table_row("SELECT (row_fun()).*")]
fn composite_value_expansion_is_not_a_source_wildcard(#[case] sql: &str) {
    let (_, context) = table_function_query(sql, true);
    assert!(context.analysis_state().wildcard_expansions().is_empty());
}

#[rstest::rstest]
#[case::insert("INSERT INTO a.zero_columns DEFAULT VALUES RETURNING *, 1 AS marker")]
#[case::delete("DELETE FROM a.zero_columns RETURNING *, 1 AS marker")]
fn returning_empty_star_preserves_group_before_fixed_output(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 1);
    assert!(groups[0].is_table_dependent());
    assert!(groups[0].columns().is_empty());
}

#[test]
fn repeated_names_keep_their_resolved_relation_occurrence() {
    let analyzed = source_namespace_statement("SELECT a.t.*, b.t.* FROM a.t CROSS JOIN b.t");
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 2);
    groups.assert_relation_schemas(&["a", "b"]);
}

#[test]
fn set_operands_keep_separate_expansion_groups() {
    let analyzed = source_namespace_statement("SELECT * FROM a.u UNION ALL SELECT * FROM b.u");
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0].columns()[0]
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        "aid"
    );
    assert_eq!(
        groups[1].columns()[0]
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        "bid"
    );
}

#[test]
fn duplicate_source_metadata_retains_columns_reordered_by_outer_using() {
    let analyzed = source_namespace_statement(
        "SELECT * FROM (a.t CROSS JOIN b.t CROSS JOIN a.u) j JOIN (SELECT 1 AS aid) v USING(aid)",
    );
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    let source = &groups[0].source_projections()[0];
    assert_eq!(source.alias(), "j");
    source
        .columns()
        .assert_retained_names("id,only_a,id,only_b,aid");
    assert_eq!(groups[0].source_ordinal_at(0), Some(("j", 4)));
    assert_eq!(groups[0].source_ordinal_at(1), Some(("j", 0)));
    assert_eq!(groups[0].source_ordinal_at(3), Some(("j", 2)));
}

#[test]
fn unaliased_same_local_relations_retain_component_qualifiers() {
    let analyzed = source_namespace_statement("SELECT * FROM a.t FULL JOIN b.t USING(id)");
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    let qualifiers = groups[0].qualifiers();
    assert_eq!(
        qualifiers
            .reference_for(
                &crate::names::FullName::with_schema("a", "t"),
                &crate::names::AliasName::from("t")
            )
            .unwrap(),
        ["a", "t"]
    );
    assert_eq!(
        qualifiers
            .reference_for(
                &crate::names::FullName::with_schema("b", "t"),
                &crate::names::AliasName::from("t")
            )
            .unwrap(),
        ["b", "t"]
    );
    assert_eq!(groups[0].reference_at(1).unwrap(), ["a", "t", "only_a"]);
}

#[test]
fn row_source_star_expands_fields_before_positional_member_access() {
    let analyzed = source_namespace_statement(
        "SELECT (ROW(j.*)).f1, (ROW(j.*)).f3 FROM (a.t CROSS JOIN b.t) j",
    );
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let columns = analyzed.ast().result_columns().unwrap();
    assert_eq!(
        columns.get_column_at(0).unwrap().col_type(),
        &Some(crate::sem::PgBuiltInType::int4())
    );
    assert_eq!(
        columns.get_column_at(1).unwrap().col_type(),
        &Some(crate::sem::PgBuiltInType::int8())
    );
    assert!(
        analyzed
            .context()
            .analysis_state()
            .wildcard_expansions()
            .is_empty()
    );
}

#[test]
fn nested_duplicate_sources_keep_alias_scopes_separate() {
    let analyzed =
        source_namespace_statement("SELECT j.* FROM ((a.t CROSS JOIN b.t) j CROSS JOIN a.u) j");
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    let outer = &groups[0].source_projections()[0];
    let inner = &outer.source_projections()[0];
    assert_eq!(outer.alias(), "j");
    assert_eq!(inner.alias(), "j");
    assert_eq!(outer.columns().len(), 5);
    assert_eq!(inner.columns().len(), 4);
    assert_eq!(outer.source_ordinal_at(0), Some(("j", 0)));
}

#[rstest::rstest]
#[case::table("SELECT j.* FROM a.mixed j(x,x)")]
#[case::function("SELECT j.* FROM mixed_fun() j(x,x)")]
#[case::view("SELECT j.* FROM a.mixed_v j(x,x)")]
fn duplicate_base_aliases_keep_physical_column_ordinals(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let groups = analyzed.context().analysis_state().wildcard_expansions();
    assert_eq!(
        groups[0].columns()[0].col_type(),
        &Some(crate::sem::PgBuiltInType::int4())
    );
    assert_eq!(
        groups[0].columns()[1].col_type(),
        &Some(crate::sem::PgBuiltInType::int8())
    );
    assert_eq!(groups[0].typed_record_at(0).unwrap().column(), "id");
    assert_eq!(groups[0].typed_record_at(1).unwrap().column(), "other");
    assert_eq!(groups[0].typed_record_at(0).unwrap().alias(), "j");
}

#[rstest::rstest]
#[case::table("SELECT j.*, $1::int4 AS parameter FROM a.mixed j(x,x)")]
#[case::function("SELECT j.*, $1::int4 AS parameter FROM mixed_fun() j(x,x)")]
fn duplicate_named_row_alias_rewrites_using_physical_fields(#[case] sql: &str) {
    let mut cache_context = create_cache_context();
    let original = source_namespace_statement(sql);
    let mut statement = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let analyzed = statement
        .analyze(original.into_context(), &mut cache_context)
        .unwrap();
    statement
        .rewrite_wildcards(analyzed.context().analysis_state().wildcard_expansions())
        .unwrap();
    let output = statement.deparse_sql(Dialect::postgres()).unwrap();
    let reparsed = parse_statements(&output, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let rewritten = reparsed
        .analyze(analyzed.into_context(), &mut cache_context)
        .unwrap();
    assert!(
        rewritten.context().reported_problems().is_empty(),
        "{:?}: {output}",
        rewritten.context().reported_problems()
    );
    let columns = rewritten.ast().result_columns().unwrap();
    assert_eq!(columns.len(), 3);
    assert_eq!(
        columns.get_column_at(0).unwrap().col_type(),
        &Some(crate::sem::PgBuiltInType::int4())
    );
    assert_eq!(
        columns.get_column_at(1).unwrap().col_type(),
        &Some(crate::sem::PgBuiltInType::int8())
    );
    assert_eq!(
        columns
            .get_column_at(0)
            .unwrap()
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        "x"
    );
    assert_eq!(
        columns
            .get_column_at(1)
            .unwrap()
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        "x"
    );
    assert!(
        rewritten
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
}

#[test]
fn hidden_duplicate_join_origin_does_not_escape_its_outer_alias() {
    let analyzed = source_namespace_statement(
        "SELECT k.* FROM ((a.t JOIN b.t ON true) j(id,id) JOIN a.u ON true) k(first,second,third)",
    );
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let group = &analyzed.context().analysis_state().wildcard_expansions()[0];
    assert_eq!(group.source_ordinal_at(0), Some(("k", 0)));
    assert_eq!(
        group.reference_at(0),
        Some(["k".to_owned(), "first".to_owned()].as_slice())
    );
}

#[test]
fn named_row_cast_does_not_accept_mismatched_field_count() {
    let analyzed = source_namespace_statement("SELECT (ROW(1)::a.mixed).id");
    assert!(!analyzed.context().reported_problems().is_empty());
}

#[rstest::rstest]
#[case::duplicate_join(
    "MERGE INTO a.u d USING (a.t CROSS JOIN b.t) j ON true WHEN MATCHED THEN DELETE RETURNING j.*"
)]
#[case::rows(
    "MERGE INTO a.u d USING ROWS FROM (mixed_fun(), mixed_fun()) r ON true WHEN MATCHED THEN DELETE RETURNING r.*"
)]
fn merge_wildcard_columns_preserve_record_roles_after_positional_binding(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let group = &analyzed.context().analysis_state().wildcard_expansions()[0];
    group.columns().assert_merge_role(MergeRecordRole::source());
}

#[rstest::rstest]
#[case::unknown_function("SELECT * FROM missing_wildcard_function()")]
#[case::invalid_arguments("SELECT * FROM row_fun(1)")]
#[case::rows_from("SELECT * FROM ROWS FROM (row_fun(1))")]
fn unresolved_function_output_is_not_a_known_empty_projection(#[case] sql: &str) {
    let (_, context) = table_function_query(sql, false);
    assert!(
        context.has_reported_problem("query_reference_not_found"),
        "{:?}",
        context.reported_problems()
    );
}

trait RetainedColumnsAssertions {
    fn assert_retained_names(&self, expected: &str);
    fn assert_merge_role(&self, expected: MergeRecordRole);
}
impl RetainedColumnsAssertions for [ResultColumn] {
    fn assert_retained_names(&self, expected: &str) {
        let names: Vec<_> = self
            .iter()
            .map(|column| column.column_name().as_ref().unwrap().as_str())
            .collect();
        assert_eq!(names.join(","), expected);
        assert!(self.iter().all(|column| column.expression().is_some()));
    }
    fn assert_merge_role(&self, expected: MergeRecordRole) {
        for column in self {
            assert_eq!(
                column
                    .expression()
                    .unwrap()
                    .merge_record_reference()
                    .unwrap()
                    .role(),
                expected
            );
        }
    }
}
trait WildcardGroupsAssertions {
    fn assert_groups(&self, locations: &[usize], dependencies: &[bool], names: &[&str]);
    fn assert_relation_schemas(&self, schemas: &[&str]);
}
impl WildcardGroupsAssertions for [WildcardExpansion] {
    fn assert_groups(&self, locations: &[usize], dependencies: &[bool], names: &[&str]) {
        for (index, group) in self.iter().enumerate() {
            assert_eq!(group.location(), Some(locations[index]));
            assert_eq!(group.is_table_dependent(), dependencies[index]);
            group.columns().assert_retained_names(names[index]);
        }
    }
    fn assert_relation_schemas(&self, schemas: &[&str]) {
        for (index, group) in self.iter().enumerate() {
            let reference = group.columns()[0]
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap();
            assert_eq!(
                reference.relation(),
                &crate::names::FullName::with_schema(schemas[index], "t")
            );
        }
    }
}
