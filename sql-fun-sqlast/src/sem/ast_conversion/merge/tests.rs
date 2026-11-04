mod assignments;
mod integration;

mod views;

use crate::sem::parameter_resolution::inference_runs as parameter_resolution_inference_runs;

use crate::test_helpers::document_merge::catalog;
use crate::test_helpers::statements::resolve_postgres as resolve;
use crate::{context::ParseContext, sem::PgBuiltInType};

#[rstest::rstest]
#[case::on_bindings("MERGE INTO items t USING incoming s ON t.id=s.sid WHEN MATCHED THEN DELETE")]
#[case::update("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid")]
#[case::insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT (id,label) VALUES(s.sid,s.slabel)"
)]
#[case::defaults(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"
)]
#[case::delete("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE")]
#[case::nothing("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DO NOTHING")]
#[case::by_source(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED BY SOURCE THEN UPDATE SET id=t.id"
)]
#[case::returning(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id,s.sid,merge_action()"
)]
#[case::star("MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING *")]
#[case::qualified_star(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.*"
)]
#[case::correlated_update(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=(SELECT s.sid)"
)]
#[case::correlated_insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES((SELECT s.sid))"
)]
#[case::returning_subquery(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING (SELECT merge_action())"
)]
#[case::row_assignment(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET (id,label)=(SELECT s.sid,s.slabel)"
)]
#[case::using_cte(
    "WITH incoming AS (SELECT 1 sid) MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
)]
#[case::using_query(
    "MERGE INTO items t USING (SELECT 1 sid) s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
)]
fn retains_merge_bindings(#[case] sql: &str) {
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
    assert_eq!(merge.clauses().len(), 1);
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
    let before = parameter_resolution_inference_runs();
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
