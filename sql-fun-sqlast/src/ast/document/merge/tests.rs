mod assignments;
mod integration;

use self::test_helpers::{catalog, resolve};
use crate::sem::{ParseContext, PgBuiltInType};

mod views;

#[rstest::rstest]
#[case::on_bindings(
    "MERGE INTO items t USING incoming s ON t.id=s.sid WHEN MATCHED THEN DELETE",
    0
)]
#[case::update(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid",
    0
)]
#[case::insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT (id,label) VALUES(s.sid,s.slabel)",
    0
)]
#[case::defaults(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES",
    0
)]
#[case::delete(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
    0
)]
#[case::nothing(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DO NOTHING",
    0
)]
#[case::by_source(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE THEN UPDATE SET id=t.id",
    0
)]
#[case::returning(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id,s.sid,merge_action()",
    3
)]
#[case::star(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING *",
    4
)]
#[case::qualified_star(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.*",
    2
)]
#[case::correlated_update(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=(SELECT s.sid)",
    0
)]
#[case::correlated_insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES((SELECT s.sid))",
    0
)]
#[case::returning_subquery(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING (SELECT merge_action())",
    1
)]
#[case::row_assignment(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET (id,label)=(SELECT s.sid,s.slabel)",
    0
)]
#[case::using_cte(
    "WITH incoming AS (SELECT 1 sid) MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid",
    0
)]
#[case::using_query(
    "MERGE INTO items t USING (SELECT 1 sid) s ON true WHEN MATCHED THEN UPDATE SET id=s.sid",
    0
)]
fn resolves_merge(#[case] sql: &str, #[case] width: usize) {
    let mut context = catalog(17);
    let statement = resolve(sql, &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let merge = statement.as_merge().unwrap();
    assert_eq!(merge.target().to_string(), "public.items");
    assert_eq!(merge.result_columns().len(), width);
    assert_eq!(merge.clauses().len(), 1);
}

#[rstest::rstest]
#[case::target_in_insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(t.id)"
)]
#[case::nested_target_in_insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES((SELECT t.id))"
)]
#[case::source_by_source(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE AND s.sid IS NULL THEN DELETE"
)]
#[case::nested_source_by_source(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE AND (SELECT s.sid IS NULL) THEN DELETE"
)]
#[case::target_in_source(
    "MERGE INTO items t USING (SELECT t.id) s ON true WHEN MATCHED THEN DELETE"
)]
#[case::hidden_alias(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=items.id"
)]
#[case::unreachable(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE WHEN MATCHED THEN DO NOTHING"
)]
#[case::nonboolean("MERGE INTO items t USING incoming s ON 1 WHEN MATCHED THEN DELETE")]
#[case::action_in_on(
    "MERGE INTO items t USING incoming s ON merge_action() IS NULL WHEN MATCHED THEN DELETE"
)]
#[case::action_in_when(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED AND merge_action() IS NULL THEN DELETE"
)]
#[case::action_in_assignment(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=merge_action()"
)]
#[case::missing_target("MERGE INTO absent t USING incoming s ON true WHEN MATCHED THEN DELETE")]
#[case::missing_column(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET absent=1"
)]
#[case::recursive(
    "WITH RECURSIVE c AS (SELECT 1) MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE"
)]
#[case::cte_unreadable(
    "WITH m AS (MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE) SELECT * FROM m"
)]
fn rejects_invalid_merge(#[case] sql: &str) {
    let mut context = catalog(17);
    let result = resolve(sql, &mut context);
    assert!(
        result.is_err() || context.reported_problem_count() > 0,
        "accepted {sql}"
    );
}

#[rstest::rstest]
#[case::basic15(
    15,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
    true
)]
#[case::basic16(
    16,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
    true
)]
#[case::old14(
    14,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE",
    false
)]
#[case::returning16(
    16,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id",
    false
)]
#[case::by_source16(
    16,
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE THEN DELETE",
    false
)]
#[case::cte16(
    16,
    "WITH m AS (MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE) SELECT 1",
    false
)]
#[case::cte17(
    17,
    "WITH m AS (MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id) SELECT id FROM m",
    true
)]
#[case::old17(
    17,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING old.id",
    false
)]
#[case::old18(
    18,
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING old.id,new.id",
    true
)]
fn respects_versions(#[case] version: u32, #[case] sql: &str, #[case] accepted: bool) {
    let mut context = catalog(version);
    let result = resolve(sql, &mut context);
    assert_eq!(
        result.is_ok() && context.reported_problem_count() == 0,
        accepted,
        "{result:?} {:?}",
        context.reported_problems()
    );
}

#[test]
fn infers_parameters_across_ordered_actions_and_returning() {
    let mut context = catalog(17);
    let before = crate::sem::parameter_resolution::inference_runs();
    let statement = resolve("MERGE INTO items t USING incoming s ON $2 WHEN MATCHED AND $3 THEN UPDATE SET id=$1 WHEN NOT MATCHED THEN INSERT(id) VALUES($1) RETURNING $1::integer AS value", &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(
        crate::sem::parameter_resolution::inference_runs(),
        before + 1
    );
    assert!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int4())
    );
    assert!(
        context
            .parameter_constraints()
            .parameter(2)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::bool())
    );
    assert_eq!(
        statement
            .as_merge()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::int4())
    );
}

#[test]
fn returning_star_preserves_source_then_target_order() {
    let mut context = catalog(17);
    let statement = resolve(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING *",
        &mut context,
    )
    .unwrap();
    let columns = statement.as_merge().unwrap().result_columns();
    assert_eq!(
        columns
            .get_column_at(0)
            .unwrap()
            .column_name()
            .as_ref()
            .unwrap()
            .to_string(),
        "sid"
    );
    assert_eq!(
        columns
            .get_column_at(2)
            .unwrap()
            .column_name()
            .as_ref()
            .unwrap()
            .to_string(),
        "id"
    );
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::{
        AnalysisError, ColumnDefinition, ColumnName, MutSourcePathWithSpan, PgBuiltInType,
        SchemaName, SemAst,
    };
    use crate::test_helpers::{TestParseContext, source_span};
    pub(super) fn catalog(version: u32) -> TestParseContext {
        let metadata = ::serde_json::from_value(::serde_json::json!({"engine":"postgres", "dialect":"postgres", "version":version.to_string(), "search_path":["public"]})).unwrap();
        let mut context = TestParseContext::default().with_metadata(&metadata);
        context.set_source_path_with_span(&source_span());
        context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context.setup_type(PgBuiltInType::bool());
        context.setup_binary("=", "int4", "int4", "bool", true);
        for (name, id, label) in [("items", "id", "label"), ("incoming", "sid", "slabel")] {
            context.setup_table(
                name,
                &[
                    ColumnDefinition::new(
                        &Some(ColumnName::from(id)),
                        Some(&PgBuiltInType::int4()),
                        Some(false),
                        &[],
                    ),
                    ColumnDefinition::new(
                        &Some(ColumnName::from(label)),
                        Some(&PgBuiltInType::text()),
                        Some(false),
                        &[],
                    ),
                ],
                &source_span(),
            );
        }
        context
    }
    pub(super) fn resolve(
        sql: &str,
        context: &mut TestParseContext,
    ) -> Result<SemAst, AnalysisError> {
        crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve(context)
    }
}
