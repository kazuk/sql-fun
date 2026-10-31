mod assignments;
mod cursors;
mod views;

use self::test_helpers::{analyze, catalog};
use crate::ast::Dialect;
use crate::context::ParseContext;
use crate::sem::PgBuiltInType;

#[rstest::rstest]
#[case::literal("UPDATE items SET id=2 RETURNING *")]
#[case::only("UPDATE ONLY items SET id=2 RETURNING id")]
#[case::default("UPDATE items SET id=DEFAULT RETURNING id")]
#[case::alias("UPDATE items AS i SET id=i.id RETURNING i.id AS changed")]
#[case::simultaneous("UPDATE items SET id=id,label=label")]
#[case::row("UPDATE items SET (id,label)=(2,'two') RETURNING id")]
#[case::row_query("UPDATE items SET (id,label)=(SELECT 2,'two') RETURNING id")]
#[case::from("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING s.n")]
#[case::target_star("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING *")]
#[case::source_star("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING s.*")]
#[case::cte("WITH s AS (SELECT 2 n) UPDATE items SET id=s.n FROM s RETURNING id")]
#[case::subquery("UPDATE items SET id=(SELECT items.id)")]
fn retains_update_bindings(#[case] sql: &str) {
    let (statement, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let update = statement.as_update().unwrap();
    assert_eq!(update.target().to_string(), "public.items");
    assert!(!update.assignments().is_empty());
    assert!(update.assignments()[0].destination().get_type().is_some());
}

#[rstest::rstest]
#[case::set("UPDATE items SET id=$1")]
#[case::cte("WITH s AS (SELECT $1::integer n) UPDATE items SET id=s.n FROM s")]
#[case::subquery("UPDATE items SET id=(SELECT $1::integer)")]
#[case::row_subquery("UPDATE items SET (id,label)=(SELECT $1::integer,'two')")]
#[case::cte_returning("WITH u AS (UPDATE items SET id=$1 RETURNING id) SELECT id FROM u")]
fn infers_parameters(#[case] sql: &str) {
    let (_, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let constraint = context.parameter_constraints().parameter(1).unwrap();
    assert!(constraint.constraint().allows(&PgBuiltInType::int4()));
    assert!(!constraint.constraint().allows(&PgBuiltInType::bool()));
}

#[rstest::rstest]
#[case::missing_relation("UPDATE absent SET id=1", false)]
#[case::missing_column("UPDATE items SET absent=1", false)]
#[case::row_width("UPDATE items SET (id,label)=(SELECT 1)", true)]
#[case::unreadable("WITH u AS (UPDATE items SET id=1) SELECT * FROM u", true)]
fn rejects_invalid_structure(#[case] sql: &str, #[case] fatal: bool) {
    let mut context = catalog(Dialect::postgres());
    let statement = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let result = statement.node.resolve(
        &mut context,
        &mut crate::test_helpers::create_cache_context(),
    );
    if fatal {
        assert!(result.is_err());
    } else {
        assert!(matches!(
            result.unwrap(),
            crate::sem::SemAst::UnresolvedStatement
        ));
        assert!(context.reported_problem_count() > 0);
    }
}

#[test]
fn preserves_default_and_resets_parameters() {
    let mut context = catalog(Dialect::postgres());
    for sql in ["UPDATE items SET id=$1", "UPDATE items SET id=DEFAULT"] {
        let statement = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let result = statement
            .node
            .resolve(
                &mut context,
                &mut crate::test_helpers::create_cache_context(),
            )
            .unwrap();
        if sql.ends_with("DEFAULT") {
            assert!(
                result.as_update().unwrap().assignments()[0]
                    .value()
                    .is_default()
            );
            assert!(context.parameter_constraints().parameter(1).is_none());
        }
    }
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::plain("UPDATE items SET id=1 RETURNING *")]
#[case::duplicate("UPDATE items SET id=1,id=2 RETURNING id")]
#[case::row("UPDATE items SET (id,label)=(2,'two') RETURNING id")]
#[case::from("UPDATE items SET id=s.n FROM (SELECT 2 n) s RETURNING id")]
#[case::original_name("UPDATE items AS i SET id=2 RETURNING items.id")]
#[case::conflict("UPDATE OR FAIL items SET id=2 RETURNING id")]
#[case::not_indexed("UPDATE items NOT INDEXED SET id=2 RETURNING id")]
fn sqlite_update(#[case] sql: &str) {
    test_helpers::sqlite_accepts(sql);
    let (statement, context) = analyze(sql, Dialect::sqlite());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(!statement.as_update().unwrap().result_columns().is_empty());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::alias("UPDATE items AS i SET id=1 RETURNING i.id")]
#[case::from("UPDATE items SET id=s.n FROM (SELECT 1 n) s RETURNING s.n")]
#[case::qualified_star("UPDATE items SET id=1 RETURNING items.*")]
fn sqlite_invalid_returning(#[case] sql: &str) {
    let (_, context) = analyze(sql, Dialect::sqlite());
    assert!(context.reported_problem_count() > 0);
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::test_helpers::{TestParseContext, source_span};
    use crate::{
        context::MutSourcePathWithSpan,
        sem::{ColumnDefinition, ColumnName, PgBuiltInType, SchemaName, SemAst},
    };

    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite_accepts(sql: &str) {
        let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
        oracle
            .execute_batch("CREATE TABLE items(id INTEGER, label TEXT);")
            .unwrap();
        assert!(oracle.prepare(sql).is_ok(), "SQLite rejected {sql}");
    }
    pub(super) fn catalog(dialect: Dialect) -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(dialect);
        context.set_source_path_with_span(&source_span());
        context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context.setup_type(PgBuiltInType::bool());
        context.setup_table(
            "items",
            &[
                ColumnDefinition::new(
                    &Some(ColumnName::from("id")),
                    Some(&PgBuiltInType::int4()),
                    Some(false),
                    &[],
                ),
                ColumnDefinition::new(
                    &Some(ColumnName::from("label")),
                    Some(&PgBuiltInType::text()),
                    Some(false),
                    &[],
                ),
            ],
            &source_span(),
        );
        context
    }
    pub(super) fn analyze(sql: &str, dialect: Dialect) -> (SemAst, TestParseContext) {
        let mut context = catalog(dialect);
        let statement = crate::parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap();
        let result = statement
            .node
            .resolve(
                &mut context,
                &mut crate::test_helpers::create_cache_context(),
            )
            .unwrap();
        (result, context)
    }
}

#[test]
fn public_postgres_entry_points_share_update_resolution() {
    use crate::{
        PostgresDialect,
        sem::{SemAst, analyze_update_statement},
        syn::{ListOpt, Opt},
    };
    let sql = "UPDATE items SET id=$1 RETURNING id";
    let parsed = PostgresDialect::parse(sql).unwrap();
    let tokens = PostgresDialect::scan(sql).unwrap();
    let result = SemAst::semantic_analysis(
        test_helpers::catalog(Dialect::postgres()),
        parsed.clone(),
        &tokens,
    )
    .unwrap();
    assert_eq!(result.ast().as_update().unwrap().result_columns().len(), 1);
    assert!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
    let update = parsed
        .get_stmts()
        .get(0)
        .get_stmt()
        .as_update_stmt()
        .as_inner()
        .unwrap();
    let (result, context) =
        analyze_update_statement(test_helpers::catalog(Dialect::postgres()), &None, update)
            .unwrap();
    assert_eq!(result.as_update().unwrap().result_columns().len(), 1);
    assert!(context.parameter_constraints().parameter(1).is_some());
}

#[cfg(feature = "sqlite")]
#[test]
fn public_sqlite_entry_point_analyzes_update() {
    let syntax = crate::test_helpers::SqliteSynBuilder
        .parse_as_stmt("UPDATE OR IGNORE items NOT INDEXED SET id=?1 RETURNING id");
    let result = crate::sem::SemAst::semantic_analysis_sqlite_stmt(
        catalog(Dialect::sqlite()),
        syntax,
        &::sql_fun_core::IVec::default(),
    )
    .unwrap();
    let update = result.ast().as_update().unwrap();
    assert!(update.or_conflict().is_some());
    assert!(update.not_indexed());
    assert_eq!(update.result_columns().len(), 1);
    assert!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
}

#[test]
fn unreferenced_update_cte_is_analyzed_in_one_inference_pass() {
    let mut context = catalog(Dialect::postgres());
    let statement = crate::parse_statements(
        "WITH u AS (UPDATE items SET id=$1) SELECT 1",
        Dialect::postgres(),
    )
    .next()
    .unwrap()
    .unwrap();
    let before = crate::sem::parameter_resolution::inference_runs();
    statement
        .node
        .resolve(
            &mut context,
            &mut crate::test_helpers::create_cache_context(),
        )
        .unwrap();
    assert_eq!(
        crate::sem::parameter_resolution::inference_runs(),
        before + 1
    );
    assert_eq!(context.reported_problem_count(), 0);
    assert!(context.parameter_constraints().parameter(1).is_some());
}

#[test]
fn inferred_types_reach_stored_assignments_predicates_and_outputs() {
    let (statement, context) = analyze(
        "UPDATE items SET id=$1 WHERE $2 RETURNING id AS n",
        Dialect::postgres(),
    );
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let update = statement.as_update().unwrap();
    assert_eq!(
        update.assignments()[0]
            .value()
            .as_expression()
            .unwrap()
            .get_type(),
        Some(PgBuiltInType::int4())
    );
    assert_eq!(
        update.predicate().unwrap().get_type(),
        Some(PgBuiltInType::bool())
    );
    assert_eq!(
        update.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
}

#[test]
fn row_assignment_retains_full_subquery_and_selected_output() {
    let (statement, context) = analyze(
        "UPDATE items SET (id,label)=(SELECT $1::integer,'two')",
        Dialect::postgres(),
    );
    assert_eq!(context.reported_problem_count(), 0);
    let update = statement.as_update().unwrap();
    let value = update.assignments()[0].value().as_query().unwrap();
    assert_eq!(value.index(), 0);
    assert_eq!(value.query().result_columns().len(), 2);
    assert_eq!(
        value
            .query()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::int4())
    );
    assert_eq!(
        update.assignments()[1].value().as_query().unwrap().index(),
        1
    );
}

#[rstest::rstest]
#[case::returning("UPDATE items SET label=$1 RETURNING $1 n")]
#[case::cte("WITH u AS (UPDATE items SET id=1 RETURNING $1 n) SELECT n FROM u")]
fn postgres_returning_defaults_unknown_parameters_to_text(#[case] sql: &str) {
    let (_, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let parameter = context.parameter_constraints().parameter(1).unwrap();
    assert!(parameter.constraint().allows(&PgBuiltInType::text()));
    assert!(!parameter.constraint().allows(&PgBuiltInType::int4()));
}

#[test]
fn nested_update_cte_reports_a_query_problem() {
    let (_, context) = analyze(
        "SELECT (WITH u AS (UPDATE items SET id=1 RETURNING id) SELECT id FROM u)",
        Dialect::postgres(),
    );
    assert!(context.reported_problem_count() > 0);
}
