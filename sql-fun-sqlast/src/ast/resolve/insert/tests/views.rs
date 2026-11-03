use crate::{ast::Dialect, context::ParseContext, names::ColumnName, sem::PgBuiltInType};

#[rstest::rstest]
#[case::values("INSERT INTO writable_items(id) VALUES (1) RETURNING *", 2)]
#[case::implicit("INSERT INTO writable_items VALUES (1, 'one') RETURNING id", 1)]
#[case::defaults("INSERT INTO writable_items DEFAULT VALUES RETURNING *", 2)]
#[case::alias("INSERT INTO writable_items AS v(id) VALUES (1) RETURNING v.*", 2)]
#[case::cte_shadow(
    "WITH writable_items AS (SELECT 9 AS other) INSERT INTO writable_items(id) VALUES (1) RETURNING *",
    2
)]
#[case::parameter("INSERT INTO writable_items(id) VALUES ($1) RETURNING id", 1)]
#[case::select("INSERT INTO writable_items(id) SELECT id FROM items RETURNING id", 1)]
#[case::inferred(
    "INSERT INTO writable_items(id) VALUES (1) ON CONFLICT(id) DO NOTHING RETURNING id",
    1
)]
#[case::nothing(
    "INSERT INTO writable_items(id) VALUES (1) ON CONFLICT DO NOTHING RETURNING *",
    2
)]
fn resolves_writable_view(#[case] sql: &str, #[case] width: usize) {
    let (statement, context) = test_helpers::analyze(sql, Dialect::postgres(), true, true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let insert = statement.unwrap();
    let insert = insert.as_insert().unwrap();
    assert_eq!(insert.target().to_string(), "public.writable_items");
    assert!(
        insert
            .view_support()
            .unwrap()
            .can_assign(&ColumnName::from("id"))
    );
    assert_eq!(insert.result_columns().len(), width);
    assert_eq!(
        insert.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
    if sql.contains("$1") {
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
    }
}

#[rstest::rstest]
#[case::read_only("INSERT INTO writable_items(id) VALUES (1)", false, false, false)]
#[case::read_only_defaults("INSERT INTO writable_items DEFAULT VALUES", false, false, false)]
#[case::column("INSERT INTO writable_items(label) VALUES ('one')", true, false, false)]
#[case::column_default(
    "INSERT INTO writable_items(label) VALUES (DEFAULT)",
    true,
    false,
    false
)]
#[case::implicit_column("INSERT INTO writable_items VALUES (1, 'one')", true, false, false)]
#[case::writable_column("INSERT INTO writable_items(id) VALUES (1)", true, false, true)]
#[case::omitted_columns("INSERT INTO writable_items DEFAULT VALUES", true, false, true)]
#[case::returning_read_only_column(
    "INSERT INTO writable_items(id) VALUES (1) RETURNING label",
    true,
    false,
    true
)]
fn checks_view_capabilities(
    #[case] sql: &str,
    #[case] writable: bool,
    #[case] all_columns: bool,
    #[case] valid: bool,
) {
    let (statement, context) =
        test_helpers::analyze(sql, Dialect::postgres(), writable, all_columns);
    assert_eq!(context.reported_problem_count() == 0, valid);
    if valid {
        assert!(statement.is_ok());
    }
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::values("INSERT INTO writable_items(id) VALUES (1) RETURNING *")]
#[case::defaults("INSERT INTO writable_items DEFAULT VALUES RETURNING id")]
fn resolves_sqlite_trigger_view(#[case] sql: &str) {
    let oracle = test_helpers::sqlite_oracle();
    assert!(oracle.prepare(sql).is_ok());
    let (statement, context) = test_helpers::analyze(sql, Dialect::sqlite(), true, true);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(
        statement
            .unwrap()
            .as_insert()
            .unwrap()
            .view_support()
            .unwrap()
            .is_writable()
    );
}

#[rstest::rstest]
#[case::rule_returning(
    "INSERT INTO writable_items(id) VALUES(1) RETURNING * /* rule */",
    Dialect::postgres()
)]
#[case::rule_conflict(
    "INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING /* rule */",
    Dialect::postgres()
)]
fn rejects_unsupported_view_operation(#[case] sql: &str, #[case] dialect: Dialect) {
    let (statement, context) = test_helpers::analyze(sql, dialect, true, true);
    assert!(matches!(
        statement.unwrap(),
        crate::sem::SemAst::UnresolvedStatement
    ));
    assert!(context.reported_problems().iter().any(|problem| {
        problem.insert_problem().is_some_and(|problem| {
            problem.kind() == crate::sem::problem::InsertProblemKind::ViewOperation
        })
    }));
}

#[rstest::rstest]
#[case::explicit("INSERT INTO writable_items(id) VALUES(1) /* identity */", false)]
#[case::default("INSERT INTO writable_items(id) VALUES(DEFAULT) /* identity */", true)]
#[case::overriding(
    "INSERT INTO writable_items(id) OVERRIDING SYSTEM VALUE VALUES(1) /* identity */",
    true
)]
fn validates_catalog_view_assignment_metadata(#[case] sql: &str, #[case] valid: bool) {
    let (statement, context) = test_helpers::analyze(sql, Dialect::postgres(), true, true);
    assert!(statement.is_ok());
    assert_eq!(context.reported_problem_count() == 0, valid);
}

#[cfg(feature = "sqlite")]
#[test]
fn rejects_sqlite_view_upsert() {
    let oracle = test_helpers::sqlite_oracle();
    assert!(
        oracle
            .prepare("INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING")
            .is_err()
    );
    let (statement, context) = test_helpers::analyze(
        "INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING",
        Dialect::sqlite(),
        true,
        true,
    );
    assert!(matches!(
        statement.unwrap(),
        crate::sem::SemAst::UnresolvedStatement
    ));
    assert_eq!(context.reported_problem_count(), 1);
}

use crate::test_helpers::insert_views as test_helpers;
