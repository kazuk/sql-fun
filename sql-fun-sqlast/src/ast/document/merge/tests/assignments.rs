use super::test_helpers::{catalog, resolve};
use crate::sem::{
    ColumnDefinition, ColumnName, FullName, MergeRecordRole, MergeStatement, PgBuiltInType,
    TableName,
};

#[rstest::rstest]
#[case::source("s.sid", true)]
#[case::target("t.id", false)]
#[case::nested_target("(SELECT t.id)", false)]
fn insert_target_subscripts_use_when_visibility(#[case] index: &str, #[case] accepted: bool) {
    let mut context = array_catalog();
    let sql = format!(
        "MERGE INTO arrays t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(a[{index}]) VALUES(1)"
    );
    let result = resolve(&sql, &mut context);
    assert_eq!(
        result.is_ok() && context.reported_problem_count() == 0,
        accepted,
        "{result:?} {:?}",
        context.reported_problems()
    );
}

fn array_catalog() -> crate::test_helpers::TestParseContext {
    let mut context = catalog(17);
    let array = PgBuiltInType::int4().array_type();
    context.setup_type(array.clone());
    context.setup_table(
        "arrays",
        &[
            ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[],
            ),
            ColumnDefinition::new(&Some(ColumnName::from("a")), Some(&array), Some(false), &[]),
        ],
        &crate::test_helpers::source_span(),
    );
    context
}

#[rstest::rstest]
#[case::identity_update_default(
    "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=DEFAULT"
)]
#[case::generated_update_default(
    "MERGE INTO generated_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET g=DEFAULT"
)]
#[case::generated_insert_default(
    "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(DEFAULT)"
)]
#[case::insert_default_values(
    "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"
)]
fn accepts_shared_default_assignment_rules(#[case] sql: &str) {
    let mut context = assignment_catalog(17);
    let statement = resolve(sql, &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(statement.as_merge().is_some());
}

#[test]
fn preserves_update_and_insert_default_values() {
    let mut context = assignment_catalog(17);
    let update = resolve(
        "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=DEFAULT",
        &mut context,
    )
    .unwrap();
    let assignment = update.as_merge().unwrap().clauses()[0]
        .action()
        .as_update()
        .unwrap()
        .assignments()[0]
        .value();
    assert!(assignment.is_default());

    let insert = resolve(
        "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(DEFAULT)",
        &mut context,
    )
    .unwrap();
    let values = insert.as_merge().unwrap().clauses()[0]
        .action()
        .as_insert()
        .unwrap()
        .values()
        .unwrap();
    assert_eq!(values.len(), 1);
    assert!(values[0].is_default());

    let defaults = resolve(
        "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES",
        &mut context,
    )
    .unwrap();
    assert!(
        defaults.as_merge().unwrap().clauses()[0]
            .action()
            .as_insert()
            .unwrap()
            .values()
            .is_none()
    );
}

#[rstest::rstest]
#[case::identity_update_value(
    "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
)]
#[case::generated_update_value(
    "MERGE INTO generated_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET g=s.sid"
)]
#[case::identity_insert_value(
    "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)"
)]
#[case::generated_insert_value(
    "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(s.sid)"
)]
fn rejects_shared_identity_and_generated_value_rules(#[case] sql: &str) {
    assert_rejected(sql, 17);
}

#[rstest::rstest]
#[case::system("SYSTEM", "system")]
#[case::user("USER", "user")]
fn retains_insert_overriding_mode(#[case] mode: &str, #[case] semantic_mode: &str) {
    let mut context = assignment_catalog(17);
    let sql = format!(
        "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) OVERRIDING {mode} VALUE VALUES(s.sid)"
    );
    let statement = resolve(&sql, &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let insert = statement.as_merge().unwrap().clauses()[0]
        .action()
        .as_insert()
        .unwrap();
    assert_eq!(insert.overriding(), Some(semantic_mode));
}

#[rstest::rstest]
#[case::duplicate_update(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid,id=s.sid"
)]
#[case::duplicate_insert(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id,id) VALUES(s.sid,s.sid)"
)]
#[case::update_type_mismatch(
    "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.slabel"
)]
#[case::insert_type_mismatch(
    "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.slabel)"
)]
fn rejects_duplicate_targets_and_type_mismatches(#[case] sql: &str) {
    assert_rejected(sql, 17);
}

#[test]
fn reports_parameter_conflicts_across_when_actions() {
    assert_rejected(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=$1 WHEN NOT MATCHED THEN INSERT(label) VALUES($1)",
        17,
    );
}

#[test]
fn serializes_typed_update_and_insert_actions() {
    let mut context = catalog(17);
    let statement = resolve(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel)",
        &mut context,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let merge = statement.as_merge().unwrap();
    let update = merge.clauses()[0].action().as_update().unwrap();
    assert_eq!(
        update.assignments()[0]
            .destination()
            .merge_record_reference()
            .unwrap()
            .role(),
        MergeRecordRole::target()
    );
    assert!(merge.clauses()[1].action().as_insert().is_some());

    let serialized = ::serde_json::to_value(merge).unwrap();
    let clauses = serialized["clauses"].as_array().unwrap();
    assert!(clauses[0]["action"].get("Update").is_some());
    assert!(clauses[1]["action"].get("Insert").is_some());

    let restored: MergeStatement = ::serde_json::from_value(serialized).unwrap();
    assert!(restored.clauses()[0].action().as_update().is_some());
    let insert = restored.clauses()[1].action().as_insert().unwrap();
    assert_eq!(insert.targets().len(), 2);
    assert_eq!(insert.values().unwrap().len(), 2);
}

#[rstest::rstest]
#[case::postgres17(17)]
#[case::postgres18(18)]
fn source_nullability_is_retained_across_postgres_versions(#[case] version: u32) {
    let mut context = strict_catalog(version);
    let statement = resolve(
        "MERGE INTO strict_items t USING (SELECT 1 AS sid) s ON t.id=s.sid WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED BY SOURCE THEN DELETE RETURNING s.sid,t.id",
        &mut context,
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let columns = statement.as_merge().unwrap().result_columns();
    let source = columns.get_column_at(0).unwrap();
    let target = columns.get_column_at(1).unwrap();
    assert_eq!(source.is_not_null(), &Some(false));
    assert_eq!(target.is_not_null(), &Some(true));
    assert_eq!(
        source
            .expression()
            .unwrap()
            .merge_record_reference()
            .unwrap()
            .role(),
        MergeRecordRole::source()
    );
    assert_eq!(
        target
            .expression()
            .unwrap()
            .merge_record_reference()
            .unwrap()
            .role(),
        MergeRecordRole::target()
    );
}

#[rstest::rstest]
#[case::update_only("WHEN MATCHED THEN UPDATE SET id=s.sid", Some(true), Some(true))]
#[case::insert_possible(
    "WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)",
    Some(false),
    Some(true)
)]
#[case::delete_possible("WHEN MATCHED THEN DELETE", Some(true), Some(false))]
fn postgres18_row_images_keep_action_nullability(
    #[case] action: &str,
    #[case] old_not_null: Option<bool>,
    #[case] new_not_null: Option<bool>,
) {
    let mut context = strict_catalog(18);
    let sql = format!(
        "MERGE INTO strict_items t USING (SELECT 1 AS sid) s ON t.id=s.sid {action} RETURNING old.id,new.id"
    );
    let statement = resolve(&sql, &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let columns = statement.as_merge().unwrap().result_columns();
    let old = columns.get_column_at(0).unwrap();
    let new = columns.get_column_at(1).unwrap();
    assert_eq!(old.is_not_null(), &old_not_null);
    assert_eq!(new.is_not_null(), &new_not_null);
    assert_eq!(
        old.expression()
            .unwrap()
            .merge_record_reference()
            .unwrap()
            .role(),
        MergeRecordRole::old_row()
    );
    assert_eq!(
        new.expression()
            .unwrap()
            .merge_record_reference()
            .unwrap()
            .role(),
        MergeRecordRole::new_row()
    );
    assert_eq!(
        old.expression()
            .unwrap()
            .merge_record_reference()
            .unwrap()
            .source()
            .get_column_name(),
        &ColumnName::from("id")
    );
}

fn assert_rejected(sql: &str, version: u32) {
    let mut context = catalog(version);
    let result = resolve(sql, &mut context);
    assert!(
        result.is_err() || context.reported_problem_count() > 0,
        "accepted {sql}"
    );
}

fn assignment_catalog(version: u32) -> crate::test_helpers::TestParseContext {
    let mut context = catalog(version);
    let identity = crate::sem::IdentityColumnConstraint::new(
        &TableName::from(FullName::with_schema("public", "identity_items")),
        &ColumnName::from("id"),
        crate::sem::IdentityColumnMode::always(),
    );
    context.setup_table(
        "identity_items",
        &[ColumnDefinition::new(
            &Some(ColumnName::from("id")),
            Some(&PgBuiltInType::int4()),
            Some(true),
            &[crate::sem::ColumnConstraint::identity(identity)],
        )],
        &crate::test_helpers::source_span(),
    );

    let generated = crate::sem::GeneratedColumnConstraint::new(
        &TableName::from(FullName::with_schema("public", "generated_items")),
        &ColumnName::from("g"),
        crate::sem::ScalarConstExpr::null(),
        Some(crate::sem::GeneratedColumnMode::stored()),
    );
    context.setup_table(
        "generated_items",
        &[ColumnDefinition::new(
            &Some(ColumnName::from("g")),
            Some(&PgBuiltInType::int4()),
            Some(false),
            &[crate::sem::ColumnConstraint::generated(generated)],
        )],
        &crate::test_helpers::source_span(),
    );
    context
}

fn strict_catalog(version: u32) -> crate::test_helpers::TestParseContext {
    let mut context = catalog(version);
    context.setup_table(
        "strict_items",
        &[ColumnDefinition::new(
            &Some(ColumnName::from("id")),
            Some(&PgBuiltInType::int4()),
            Some(true),
            &[],
        )],
        &crate::test_helpers::source_span(),
    );
    context
}
