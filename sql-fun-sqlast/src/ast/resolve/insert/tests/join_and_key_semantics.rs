use crate::ast::Dialect;
use crate::sem::{ColumnName, PgBuiltInType};

#[rstest::rstest]
#[case::on(
    "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 'one'::text label) b ON true) AS j RETURNING *",
    2
)]
#[case::using(
    "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 1 id) b USING(id)) AS j RETURNING id",
    1
)]
#[case::natural(
    "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a NATURAL JOIN (SELECT 1 id) b) AS j RETURNING id",
    1
)]
#[case::left(
    "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a LEFT JOIN (SELECT 'one'::text label) b ON false) AS j RETURNING *",
    2
)]
fn expands_qualified_join_star(#[case] sql: &str, #[case] width: usize) {
    let (statement, context) = super::test_helpers::analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let insert = statement.as_insert().unwrap();
    assert_eq!(insert.columns().len(), width);
    assert_eq!(insert.result_columns().len(), width);
    assert_eq!(
        insert.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
}

#[rstest::rstest]
#[case::primary("id int4 PRIMARY KEY", true, false)]
#[case::named_primary("id int4 CONSTRAINT key579 PRIMARY KEY", true, true)]
#[case::unique("id int4 UNIQUE", false, false)]
#[case::named_unique("id int4 CONSTRAINT key579 UNIQUE", false, true)]
#[case::table_primary("id int4, CONSTRAINT key579 PRIMARY KEY(id)", true, true)]
#[case::table_unique("id int4, CONSTRAINT key579 UNIQUE(id)", false, true)]
fn retains_one_arbiter_and_correct_key_nullability(
    #[case] definition: &str,
    #[case] not_null: bool,
    #[case] named: bool,
    #[values("id", "*")] returning: &str,
) {
    let (statement, context) = test_helpers::analyze_keyed(definition, returning);
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let insert = statement.as_insert().unwrap();
    let column = insert.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.column_name(), &Some(ColumnName::from("id")));
    assert_eq!(column.is_not_null(), &Some(not_null));
    let arbiters = insert.conflicts()[0].arbiter().arbiters().unwrap();
    assert_eq!(arbiters.len(), 1);
    if named {
        assert_eq!(arbiters[0].constraint().unwrap().name(), "key579");
        assert!(arbiters[0].column().is_none());
    } else {
        assert_eq!(arbiters[0].column(), Some(&ColumnName::from("id")));
    }
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::SemAst;
    use crate::test_helpers::TestParseContext;

    pub(super) fn analyze_keyed(definition: &str, returning: &str) -> (SemAst, TestParseContext) {
        let mut cache_context = crate::test_helpers::create_cache_context();
        let dialect = Dialect::postgres();
        let mut context =
            crate::parse_statements(&format!("CREATE TABLE review579({definition})"), dialect)
                .next()
                .unwrap()
                .unwrap()
                .analyze(
                    super::super::test_helpers::catalog(dialect),
                    &mut cache_context,
                )
                .unwrap()
                .into_context();
        let statement = crate::parse_statements(
            &format!(
                "INSERT INTO review579 VALUES(1) ON CONFLICT(id) DO NOTHING RETURNING {returning}"
            ),
            dialect,
        )
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(
            &mut context,
            &mut crate::test_helpers::create_cache_context(),
        )
        .unwrap();
        (statement, context)
    }
}
