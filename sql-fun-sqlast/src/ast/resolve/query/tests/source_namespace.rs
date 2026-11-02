#[cfg(feature = "sqlite")]
use super::super::test_helpers::resolve;
use super::super::test_helpers::source_namespace_statement;
#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
use crate::sem::{ColumnReferenceExpr, FullName, PgBuiltInType, SemScalarExpr};

#[rstest::rstest]
#[case::comma("SELECT a.t.id, b.t.id FROM a.t, b.t")]
#[case::reversed("SELECT a.t.id, b.t.id FROM b.t, a.t")]
#[case::cross_join("SELECT a.t.id, b.t.id FROM a.t CROSS JOIN b.t")]
#[case::inner_join("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON true")]
#[case::search_path("SELECT a.t.id, b.t.id FROM t, b.t")]
#[case::join_on("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON a.t.id IS NOT NULL")]
fn distinct_relations_bind_without_problems(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, schema, ty) in [
        (0, "a", PgBuiltInType::int4()),
        (1, "b", PgBuiltInType::int8()),
    ] {
        let column = query.result_columns().get_column_at(index).unwrap();
        assert_eq!(
            column
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema(schema, "t")
        );
        assert_eq!(column.col_type(), &Some(ty));
    }
}

#[rstest::rstest]
#[case::second("SELECT only_b FROM a.t, b.t", "b", "t", PgBuiltInType::int8())]
#[case::first("SELECT only_b FROM b.t, a.t", "b", "t", PgBuiltInType::int8())]
#[case::join(
    "SELECT only_b FROM a.t CROSS JOIN b.t",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::quoted(
    r#"SELECT "b.b"."t.x".id FROM "a.b"."t.x", "b.b"."t.x""#,
    "b.b",
    "t.x",
    PgBuiltInType::int8()
)]
#[case::outer(
    "SELECT (SELECT only_b FROM a.u) FROM a.t, b.t",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::join_alias(
    "SELECT j.only_b FROM (a.t CROSS JOIN b.t) AS j",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::join_alias_bare(
    "SELECT only_b FROM (a.t CROSS JOIN b.t) AS j",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::join_alias_renamed(
    "SELECT j.z FROM (a.t CROSS JOIN b.t) AS j(x,y,z,w)",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::hidden_namespace(
    "SELECT b.t.id FROM (a.t CROSS JOIN a.u) AS j, b.t",
    "b",
    "t",
    PgBuiltInType::int8()
)]
#[case::outer_qualifier(
    "SELECT (SELECT t.id FROM a.u) FROM b.t",
    "b",
    "t",
    PgBuiltInType::int8()
)]
fn unique_column_retains_its_occurrence(
    #[case] sql: &str,
    #[case] schema: &str,
    #[case] table: &str,
    #[case] ty: crate::sem::TypeReference,
) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let mut bindings = test_helpers::TableBindings::default();
    query.walk(&mut bindings).unwrap();
    bindings.assert_only_relation(&FullName::with_schema(schema, table));
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(ty)
    );
}

#[rstest::rstest]
#[case::common_column("SELECT t.id FROM a.t, b.t")]
#[case::unique_column("SELECT t.only_b FROM a.t, b.t")]
#[case::unique_other_order("SELECT t.only_b FROM b.t, a.t")]
#[case::join("SELECT t.id FROM a.t CROSS JOIN b.t")]
#[case::star("SELECT t.* FROM a.t, b.t")]
#[case::local_blocks_outer("SELECT (SELECT t.id FROM a.t, b.t) FROM a.t AS t")]
fn local_source_ambiguity_is_retained(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    let problems = analyzed.context().reported_problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.code() == "column_source_ambiguous"),
        "{problems:?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.code() == "duplicate_alias")
    );
    let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let mut failures = test_helpers::SourceFailures::default();
    query.walk(&mut failures).unwrap();
    failures.assert_source_ambiguity();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
}

#[rstest::rstest]
#[case::comma("SELECT id FROM a.t, b.t")]
#[case::join("SELECT id FROM a.t CROSS JOIN b.t")]
#[case::joined_alias("SELECT j.id FROM (a.t CROSS JOIN b.t) AS j")]
fn bare_column_ambiguity_is_retained(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "ambiguous_column")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert!(matches!(
        column.expression(),
        Some(SemScalarExpr::ColumnRef(
            ColumnReferenceExpr::AmbiguousColumnRef(_)
        ))
    ));
    assert!(column.col_type().is_none());
}

#[rstest::rstest]
#[case::identical("SELECT 1 FROM a.t, a.t")]
#[case::different_spelling("SELECT 1 FROM a.t, t")]
#[case::explicit_same("SELECT 1 FROM a.t AS t, b.t")]
#[case::explicit_right("SELECT 1 FROM a.t, b.t AS t")]
#[case::both_explicit("SELECT 1 FROM a.t AS x, b.t AS x")]
#[case::cte("WITH t AS (SELECT 1 AS id) SELECT 1 FROM t, a.t")]
#[case::derived("SELECT 1 FROM (SELECT 1 AS id) AS t, b.t")]
#[case::join("SELECT 1 FROM a.t CROSS JOIN a.t")]
#[case::repeated_view("SELECT 1 FROM a.v, a.v")]
#[case::update("UPDATE a.t SET id=1 FROM a.t")]
#[case::delete("DELETE FROM a.t USING a.t")]
#[case::merge("MERGE INTO a.t USING b.t ON true WHEN MATCHED THEN DELETE")]
#[case::merge_ambiguous_source(
    "MERGE INTO a.t USING (b.t CROSS JOIN c.t) ON true WHEN MATCHED THEN DELETE"
)]
fn genuine_source_collisions_remain_invalid(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "duplicate_alias"),
        "{:?}",
        analyzed.context().reported_problems()
    );
}

#[rstest::rstest]
#[case::comma("SELECT * FROM a.t, b.t", &["a", "a", "b", "b"])]
#[case::reversed("SELECT * FROM b.t, a.t", &["b", "b", "a", "a"])]
#[case::schema_qualified("SELECT a.t.* FROM a.t, b.t", &["a", "a"])]
#[case::join("SELECT * FROM a.t CROSS JOIN b.t", &["a", "a", "b", "b"])]
#[case::left("SELECT * FROM a.t LEFT JOIN b.t ON true", &["a", "a", "b", "b"])]
#[case::right("SELECT * FROM a.t RIGHT JOIN b.t ON true", &["a", "a", "b", "b"])]
#[case::update("UPDATE a.t SET id=1 FROM b.t RETURNING *", &["a", "a"])]
#[case::delete("DELETE FROM a.t USING b.t RETURNING *", &["a", "a"])]
fn star_preserves_occurrences_and_output_order(#[case] sql: &str, #[case] schemas: &[&str]) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let columns = test_helpers::result_columns(analyzed.ast());
    assert_eq!(columns.len(), schemas.len());
    for (index, schema) in schemas.iter().enumerate() {
        let column = columns.get_column_at(index).unwrap();
        assert_eq!(
            column
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema(schema, "t")
        );
        assert_eq!(
            column.col_type(),
            &Some(if *schema == "a" {
                PgBuiltInType::int4()
            } else {
                PgBuiltInType::int8()
            })
        );
        let null_extended = (sql.contains("LEFT JOIN") && *schema == "b")
            || (sql.contains("RIGHT JOIN") && *schema == "a");
        assert_eq!(column.is_not_null(), &Some(!null_extended));
    }
}

#[rstest::rstest]
#[case::views("SELECT a.v.id, b.v.id FROM a.v, b.v")]
#[case::mixed("SELECT a.t.id, c.t.id FROM a.t, c.t")]
fn views_share_the_relation_collision_rule(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
    assert_eq!(
        query.result_columns().get_column_at(1).unwrap().col_type(),
        &Some(PgBuiltInType::int8())
    );
}

#[rstest::rstest]
#[case::left("SELECT t.* FROM a.u LEFT JOIN b.t ON true", false)]
#[case::right("SELECT t.* FROM b.t RIGHT JOIN a.u ON true", false)]
#[case::inner("SELECT t.* FROM a.u CROSS JOIN b.t", true)]
#[case::unique("SELECT only_b FROM a.t LEFT JOIN b.t ON true", false)]
fn selected_occurrence_keeps_join_nullability(#[case] sql: &str, #[case] not_null: bool) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
    assert_eq!(column.is_not_null(), &Some(not_null));
    assert_eq!(
        column
            .expression()
            .unwrap()
            .table_column_reference()
            .unwrap()
            .relation(),
        &FullName::with_schema("b", "t")
    );
}

#[rstest::rstest]
#[case::inner("SELECT * FROM a.t JOIN b.t USING(id)", &["a", "a", "b"])]
#[case::right("SELECT * FROM a.t RIGHT JOIN b.t USING(id)", &["b", "a", "b"])]
#[case::natural("SELECT * FROM a.t NATURAL JOIN b.t", &["a", "a", "b"])]
#[case::aliased("SELECT j.* FROM (a.t CROSS JOIN b.t) j", &["a", "a", "b", "b"])]
fn joined_star_keeps_selected_sources(#[case] sql: &str, #[case] schemas: &[&str]) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let columns = query.result_columns();
    assert_eq!(columns.len(), schemas.len());
    for (index, schema) in schemas.iter().enumerate() {
        assert_eq!(
            columns
                .get_column_at(index)
                .unwrap()
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema(schema, "t")
        );
    }
}

#[rstest::rstest]
#[case::star("SELECT * FROM a.t FULL JOIN b.t USING(id)")]
#[case::explicit("SELECT id, only_a, only_b FROM a.t FULL JOIN b.t USING(id)")]
#[case::aliased("SELECT j.* FROM (a.t FULL JOIN b.t USING(id)) AS j")]
fn full_join_using_keeps_merged_key_and_order(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let columns = query.result_columns();
    assert_eq!(
        columns
            .names()
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        ["id", "only_a", "only_b"]
    );
    let merged = columns.get_column_at(0).unwrap();
    assert_eq!(merged.col_type(), &Some(PgBuiltInType::int8()));
    assert_eq!(merged.is_not_null(), &Some(true));
    let coalesce = merged.expression().unwrap().as_coalesce().unwrap();
    assert_eq!(coalesce.arguments().len(), 2);
    assert_eq!(
        coalesce.arguments()[0].get_type(),
        Some(PgBuiltInType::int8())
    );
    assert_eq!(
        coalesce.arguments()[1]
            .table_column_reference()
            .unwrap()
            .relation(),
        &FullName::with_schema("b", "t")
    );
}

#[rstest::rstest]
#[case::nested("SELECT * FROM a.t FULL JOIN b.t USING(id) FULL JOIN c.t USING(id)", &["id", "only_a", "only_b"])]
#[case::natural("SELECT * FROM a.t NATURAL FULL JOIN b.t", &["id", "only_a", "only_b"])]
fn full_join_merged_key_survives_other_join_shapes(#[case] sql: &str, #[case] names: &[&str]) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let columns = query.result_columns();
    assert_eq!(
        columns
            .names()
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        names
    );
    let key = columns.get_column_at(0).unwrap();
    assert_eq!(key.col_type(), &Some(PgBuiltInType::int8()));
    assert_eq!(key.is_not_null(), &Some(true));
    assert!(key.expression().unwrap().as_coalesce().is_some());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::bare("SELECT * FROM (SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)")]
#[case::grouped("SELECT * FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j")]
fn sqlite_full_join_using_keeps_left_position_and_right_fallback(#[case] sql: &str) {
    let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
    assert!(
        context.reported_problems().is_empty(),
        "{:?}",
        context.reported_problems()
    );
    let columns = query.result_columns();
    assert_eq!(columns.len(), 1);
    let key = columns.get_column_at(0).unwrap();
    assert!(key.expression().unwrap().as_coalesce().is_some());
    assert_eq!(key.column_name().as_ref().unwrap().as_str(), "id");
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::qualified("j.id")]
#[case::unqualified("id")]
fn sqlite_grouped_full_join_key_keeps_both_inputs(#[case] key: &str) {
    let (query, _) = resolve(
        &format!(
            "SELECT {key} FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j"
        ),
        Dialect::sqlite(),
        false,
    )
    .unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_coalesce()
            .is_some()
    );
}

#[rstest::rstest]
#[case::qualified("SELECT (SELECT t.only_b FROM a.t) FROM b.t")]
#[case::star("SELECT (SELECT t.only_b FROM a.t) FROM b.t AS t")]
fn missing_local_column_does_not_bind_outer_source(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "column_not_found_in_table")
    );
    let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let mut bindings = test_helpers::TableBindings::default();
    query.walk(&mut bindings).unwrap();
    bindings.assert_only_relation(&FullName::with_schema("a", "t"));
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
}

#[test]
fn join_alias_hides_colliding_inner_occurrences() {
    let analyzed = source_namespace_statement("SELECT a.t.id FROM (a.t CROSS JOIN a.u) j, a.t");
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .table_column_reference()
            .unwrap()
            .relation(),
        &FullName::with_schema("a", "t")
    );
}

#[test]
fn locking_a_short_ambiguous_source_retains_failure() {
    let analyzed = source_namespace_statement("SELECT a.t.id FROM a.t,b.t FOR UPDATE OF t");
    assert!(
        analyzed
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "query_reference_ambiguous"),
        "{:?}",
        analyzed.context().reported_problems()
    );
}

#[rstest::rstest]
#[case::table("SELECT j.z FROM (a.t CROSS JOIN b.t) j(x,y,z,w)")]
#[case::view("SELECT j.z FROM (a.t CROSS JOIN b.v) j(x,y,z)")]
#[case::cte("WITH c AS (SELECT id FROM b.t) SELECT j.z FROM (a.t CROSS JOIN c) j(x,y,z)")]
#[case::derived("SELECT j.z FROM (a.t CROSS JOIN (SELECT id FROM b.t) s) j(x,y,z)")]
fn joined_alias_keeps_its_exposed_column_name(#[case] sql: &str) {
    let analyzed = source_namespace_statement(sql);
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.column_name().as_ref().unwrap().to_string(), "z");
    assert_eq!(column.expression().unwrap().get_column_name(), "z");
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
}

#[test]
fn a_join_alias_preserves_occurrence_identity() {
    let analyzed =
        source_namespace_statement("SELECT j.only_a, t.only_a FROM (a.t CROSS JOIN a.u) j, a.t");
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, alias) in [(0, "j"), (1, "t")] {
        let binding = query
            .result_columns()
            .get_column_at(index)
            .unwrap()
            .expression()
            .unwrap()
            .table_column_reference()
            .unwrap();
        assert_eq!(binding.alias().as_str(), alias);
    }
}

#[rstest::rstest]
#[case::old("old", true)]
#[case::new("new", false)]
fn delete_row_images_keep_the_destination_occurrence(#[case] image: &str, #[case] not_null: bool) {
    let sql = format!("DELETE FROM a.t USING b.t RETURNING {image}.*");
    let analyzed = super::super::test_helpers::source_namespace_statement_at_version(&sql, "18");
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let columns = analyzed.ast().as_delete().unwrap().result_columns();
    assert_eq!(columns.len(), 2);
    for index in 0..columns.len() {
        let column = columns.get_column_at(index).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(not_null));
        let expression = column.expression().unwrap();
        let SemScalarExpr::ColumnRef(reference) = expression else {
            panic!("row-image column expected");
        };
        let row = reference.delete_row_reference().unwrap();
        assert_eq!(
            row.source().table_column_reference().unwrap().relation(),
            &FullName::with_schema("a", "t")
        );
    }
}

#[test]
fn grouping_prefers_the_selected_source_over_an_output_alias() {
    let analyzed =
        source_namespace_statement("SELECT 1 AS z FROM (a.t CROSS JOIN b.t) j(x,y,z,w) GROUP BY z");
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let mut bindings = test_helpers::TableBindings::default();
    query.walk(&mut bindings).unwrap();
    bindings.assert_only_relation(&FullName::with_schema("b", "t"));
}

#[test]
fn merge_projection_keeps_source_role_and_join_alias() {
    let analyzed = source_namespace_statement(
        "MERGE INTO a.u AS d USING (a.t CROSS JOIN b.t) j(x,y,z,w) ON true WHEN MATCHED THEN DELETE RETURNING j.*",
    );
    assert!(
        analyzed.context().reported_problems().is_empty(),
        "{:?}",
        analyzed.context().reported_problems()
    );
    let columns = analyzed.ast().as_merge().unwrap().result_columns();
    assert_eq!(columns.len(), 4);
    for (index, name) in ["x", "y", "z", "w"].into_iter().enumerate() {
        let column = columns.get_column_at(index).unwrap();
        assert_eq!(column.column_name().as_ref().unwrap().to_string(), name);
        assert_eq!(column.expression().unwrap().get_column_name(), name);
        let record = column
            .expression()
            .unwrap()
            .merge_record_reference()
            .unwrap();
        assert_eq!(record.role(), crate::sem::MergeRecordRole::source());
        assert_eq!(
            record
                .source()
                .table_column_reference()
                .unwrap()
                .alias()
                .as_str(),
            "j"
        );
    }
}

#[test]
fn an_unresolved_join_input_blocks_qualified_binding() {
    let analyzed = source_namespace_statement(
        "SELECT (SELECT j.id FROM (a.t CROSS JOIN missing.t) j) FROM b.t",
    );
    assert!(
        analyzed
            .context()
            .reported_problems()
            .iter()
            .any(|problem| problem.code() == "relation_not_found")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
}

mod test_helpers {
    use crate::sem::{
        AnalysisError, FullName, ResultColumnCollection, SemScalarExpr,
        traversal::SemanticVisitorMut,
    };

    pub(super) fn result_columns(ast: &crate::sem::SemAst) -> ResultColumnCollection {
        if let Some(statement) = ast.as_update() {
            statement.result_columns().clone()
        } else if let Some(statement) = ast.as_delete() {
            statement.result_columns().clone()
        } else {
            ast.clone()
                .into_select_statemtnt()
                .unwrap()
                .result_columns()
                .clone()
        }
    }

    #[derive(Default)]
    pub(super) struct TableBindings {
        relations: Vec<FullName>,
    }
    impl TableBindings {
        pub(super) fn assert_only_relation(&self, relation: &FullName) {
            assert_eq!(self.relations.as_slice(), std::slice::from_ref(relation));
        }
    }
    impl SemanticVisitorMut for TableBindings {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let Some(binding) = expression.table_column_reference() {
                self.relations.push(binding.relation().clone());
            }
            Ok(())
        }
    }
    #[derive(Default)]
    pub(super) struct SourceFailures {
        codes: Vec<&'static str>,
    }
    impl SourceFailures {
        pub(super) fn assert_source_ambiguity(&self) {
            assert_eq!(self.codes, ["column_source_ambiguous"]);
        }
    }
    impl SemanticVisitorMut for SourceFailures {
        fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
            if let Some(failure) = expression.qualification_failure() {
                self.codes.push(failure.code());
            }
            Ok(())
        }
    }
}
