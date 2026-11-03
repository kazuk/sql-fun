mod cursors;
mod sql_driven;
mod versions;
mod views;

#[cfg(feature = "sqlite")]
use crate::parse_statements;
#[cfg(feature = "sqlite")]
use crate::sem::SemAst as SemSemAst;
use crate::sem::{
    DeleteStatement, parameter_resolution::inference_runs as parameter_resolution_inference_runs,
};
#[cfg(feature = "sqlite")]
use crate::test_helpers::SqliteSynBuilder;

use crate::test_helpers::delete::analyze;
use crate::test_helpers::delete::catalog;
use crate::{
    ast::Dialect,
    context::{BaseParseContext, ParseContext},
    names::ColumnName,
    sem::PgBuiltInType,
};

#[rstest::rstest]
#[case::plain("DELETE FROM items")]
#[case::star("DELETE FROM items RETURNING *")]
#[case::only("DELETE FROM ONLY items RETURNING id")]
#[case::alias("DELETE FROM items AS i WHERE true RETURNING i.id AS removed")]
#[case::using("DELETE FROM items USING (SELECT 2 n) s RETURNING s.n")]
#[case::target_star("DELETE FROM items USING (SELECT 2 n) s RETURNING *")]
#[case::source_star("DELETE FROM items USING (SELECT 2 n) s RETURNING s.*")]
#[case::with("WITH s AS (SELECT 2 n) DELETE FROM items USING s RETURNING s.n")]
#[case::correlated(
    "DELETE FROM items WHERE (SELECT items.id IS NULL) RETURNING (SELECT items.label)"
)]
#[case::cte_shadow("WITH items AS (SELECT 2 other) DELETE FROM items RETURNING id")]
fn retains_delete_bindings(#[case] sql: &str) {
    let (statement, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let delete = statement.as_delete().unwrap();
    assert_eq!(delete.target().to_string(), "public.items");
    assert_eq!(delete.includes_descendants(), !sql.contains("ONLY"));
    assert!(
        delete
            .from()
            .resolve_alias(&crate::names::AliasName::from(if sql.contains("AS i ") {
                "i"
            } else {
                "items"
            }))
            .is_some()
    );
}

#[test]
fn nested_returning_projection_fixes_its_unknown_parameter_before_outer_casts() {
    let mut context = test_helpers::catalog_with_text_integer_cast();
    let statement = test_helpers::resolve(
        "DELETE FROM items RETURNING (SELECT $1) AS label, $1::integer AS n",
        &mut context,
        Dialect::postgres(),
    )
    .unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let constraint = context
        .parameter_constraints()
        .parameter(1)
        .unwrap()
        .constraint();
    assert!(constraint.allows(&PgBuiltInType::text()));
    assert!(!constraint.allows(&PgBuiltInType::int4()));
    assert_eq!(
        statement
            .as_delete()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::text())
    );
}

#[test]
fn preserves_result_bindings_and_metadata() {
    let (statement, context) = analyze(
        "DELETE FROM items AS i RETURNING i.id AS removed, label",
        Dialect::postgres(),
    );
    let delete = statement.as_delete().unwrap();
    let column = delete.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), &Some(true));
    assert_eq!(
        column
            .expression()
            .unwrap()
            .get_column_def()
            .unwrap()
            .name(),
        &Some(ColumnName::from("id"))
    );
    assert_eq!(
        delete
            .result_columns()
            .get_column_at(1)
            .unwrap()
            .is_not_null(),
        &Some(false)
    );
    assert!(
        context
            .get_table(&crate::names::TableName::from(
                crate::names::FullName::from_local_name("items")
            ))
            .is_some()
    );
}

#[rstest::rstest]
#[case::where_parameter("DELETE FROM items WHERE $1", true)]
#[case::returning("DELETE FROM items RETURNING $1::integer", false)]
#[case::with(
    "WITH s AS (SELECT $1::integer n) DELETE FROM items USING s RETURNING s.n",
    false
)]
#[case::nested("DELETE FROM items RETURNING (SELECT $1::integer)", false)]
#[case::cte_returning(
    "WITH d(n) AS (DELETE FROM items RETURNING $1::integer) SELECT n FROM d",
    false
)]
fn infers_parameters(#[case] sql: &str, #[case] boolean: bool) {
    let (_, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let constraint = context.parameter_constraints().parameter(1).unwrap();
    assert_eq!(
        constraint.constraint().allows(&PgBuiltInType::bool()),
        boolean
    );
    assert_eq!(
        constraint.constraint().allows(&PgBuiltInType::int4()),
        !boolean
    );
}

#[test]
fn synthetic_parameter_conflict_retains_its_diagnostic() {
    let (_, context) = analyze(
        "DELETE FROM items WHERE $1 RETURNING $1::integer",
        Dialect::postgres(),
    );
    assert!(context.reported_problem_count() > 0);
}

#[test]
fn retains_distinct_missing_and_ambiguous_lookup_outcomes() {
    let (statement, _) = analyze(
        "DELETE FROM items USING items AS other RETURNING id, absent",
        Dialect::postgres(),
    );
    let columns = statement.as_delete().unwrap().result_columns();
    let ambiguous = columns.get_column_at(0).unwrap().expression().unwrap();
    let missing = columns.get_column_at(1).unwrap().expression().unwrap();
    assert_matches::assert_matches!(
        ambiguous,
        crate::sem::SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(
            _
        ))
    );
    assert_matches::assert_matches!(
        missing,
        crate::sem::SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(_))
    );
}

#[rstest::rstest]
#[case::missing_target("DELETE FROM absent")]
#[case::unreadable("WITH d AS (DELETE FROM items) SELECT * FROM d")]
#[case::nested("SELECT (WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d)")]
fn retains_invalid_structure_as_diagnostics(#[case] sql: &str) {
    let mut context = catalog(Dialect::postgres());
    assert!(test_helpers::resolve(sql, &mut context, Dialect::postgres()).is_ok());
    assert!(context.reported_problem_count() > 0);
}

#[test]
fn cte_aliases_and_parameters_survive_a_single_outer_inference_pass() {
    let mut context = catalog(Dialect::postgres());
    let before = parameter_resolution_inference_runs();
    let result = test_helpers::resolve("WITH s AS (SELECT $1::integer n), d(removed) AS (DELETE FROM items USING s WHERE $2 RETURNING s.n) SELECT removed FROM d", &mut context, Dialect::postgres()).unwrap();
    assert_eq!(
        crate::sem::parameter_resolution::inference_runs(),
        before + 1
    );
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let query = result.into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .column_name(),
        &Some(ColumnName::from("removed"))
    );
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
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
}

#[test]
fn analyzes_unused_cte_and_resets_statement_parameter_state() {
    let mut context = catalog(Dialect::postgres());
    test_helpers::resolve(
        "WITH d AS (DELETE FROM items WHERE $1) SELECT 1",
        &mut context,
        Dialect::postgres(),
    )
    .unwrap();
    assert!(context.parameter_constraints().parameter(1).is_some());
    let result = test_helpers::resolve(
        "DELETE FROM items RETURNING id",
        &mut context,
        Dialect::postgres(),
    )
    .unwrap();
    assert!(context.parameter_constraints().parameter(1).is_none());
    // Result metadata serialization follows the existing ResultColumn contract:
    // expressions are skipped, while target, scopes and column metadata persist.
    let serialized = ::serde_json::to_value(result.as_delete().unwrap()).unwrap();
    let restored: DeleteStatement = ::serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.target(), result.as_delete().unwrap().target());
    assert_eq!(
        restored
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::int4())
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::plain("DELETE FROM items RETURNING *")]
#[case::alias("DELETE FROM items AS i WHERE i.id IS NULL RETURNING items.id")]
#[case::with(
    "WITH c AS (SELECT 1 n) DELETE FROM items WHERE EXISTS (SELECT n FROM c) RETURNING id"
)]
#[case::not_indexed("DELETE FROM items NOT INDEXED RETURNING id")]
#[case::parameter("DELETE FROM items WHERE ?1 RETURNING ?2")]
fn sqlite_delete(#[case] sql: &str) {
    test_helpers::sqlite_accepts(sql);
    let (statement, context) = analyze(sql, Dialect::sqlite());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(!statement.as_delete().unwrap().result_columns().is_empty());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::alias("DELETE FROM items AS i RETURNING i.id")]
#[case::star("DELETE FROM items RETURNING items.*")]
fn sqlite_rejects_invalid_returning(#[case] sql: &str) {
    let (_, context) = analyze(sql, Dialect::sqlite());
    assert!(context.reported_problem_count() > 0);
}

#[test]
fn postgres_public_entry_points_agree() {
    use crate::{
        PostgresDialect,
        sem::{SemAst, analyze_delete_statement},
        syn::{ListOpt, Opt},
    };
    let sql = "DELETE FROM items WHERE $1 RETURNING id";
    let parsed = PostgresDialect::parse(sql).unwrap();
    let tokens = PostgresDialect::scan(sql).unwrap();
    let result =
        SemAst::semantic_analysis(catalog(Dialect::postgres()), parsed.clone(), &tokens).unwrap();
    let delete = parsed
        .get_stmts()
        .get(0)
        .get_stmt()
        .as_delete_stmt()
        .as_inner()
        .unwrap();
    let (adapted, context) =
        analyze_delete_statement(catalog(Dialect::postgres()), &None, delete).unwrap();
    assert_eq!(result.ast().as_delete(), adapted.as_delete());
    assert_eq!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint(),
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_public_adapter_analyzes_delete() {
    let syntax =
        SqliteSynBuilder.parse_as_stmt("DELETE FROM items NOT INDEXED WHERE ?1 RETURNING id");
    let result = SemSemAst::semantic_analysis_sqlite_stmt(
        catalog(Dialect::sqlite()),
        syntax,
        &::sql_fun_core::IVec::default(),
    )
    .unwrap();
    let delete = result.ast().as_delete().unwrap();
    assert!(delete.not_indexed());
    assert_eq!(delete.result_columns().len(), 1);
    assert!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
}

use crate::test_helpers::delete as test_helpers;

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::target("items", false)]
#[case::other_target("other", true)]
fn sqlite_index_hint_belongs_to_the_target(#[case] indexed_table: &str, #[case] invalid: bool) {
    use crate::{
        names::{FullName, TableName},
        sem::{CreateIndex, IndexElementCollection},
    };
    let context = catalog(Dialect::sqlite());
    let index = CreateIndex::new(
        &FullName::with_schema("public", "items_idx"),
        &TableName::from(FullName::with_schema("public", indexed_table)),
        &IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into()),
    );
    let mut context = context.apply_create_index(&index).unwrap();
    let result = test_helpers::resolve(
        "DELETE FROM items INDEXED BY items_idx RETURNING id",
        &mut context,
        Dialect::sqlite(),
    )
    .unwrap();
    assert_eq!(context.reported_problem_count() > 0, invalid);
    assert_eq!(result.as_delete().unwrap().indexed(), Some("items_idx"));
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_missing_index_reports_a_structured_violation() {
    let (_, context) = analyze("DELETE FROM items INDEXED BY absent", Dialect::sqlite());
    assert_eq!(
        context.reported_problems()[0]
            .delete_problem()
            .unwrap()
            .kind(),
        &crate::sem::problem::DeleteProblemKind::index_hint()
    );
}

#[rstest::rstest]
#[case::missing("DELETE FROM absent WHERE $1::boolean RETURNING absent.id", false)]
#[case::missing_star("DELETE FROM absent RETURNING *", false)]
#[case::found("DELETE FROM items RETURNING id", true)]
fn retains_destination_lookup_outcome(#[case] sql: &str, #[case] found: bool) {
    let mut context = catalog(Dialect::postgres());
    let statement = test_helpers::resolve(sql, &mut context, Dialect::postgres()).unwrap();
    let delete = statement.as_delete().unwrap();
    assert_eq!(delete.target_found(), found);
    assert_eq!(context.reported_problem_count() == 0, found);
    if !found {
        assert_eq!(delete.target().to_string(), "absent");
        assert!(matches!(
            delete
                .from()
                .resolve_alias(&crate::names::AliasName::from("absent"))
                .unwrap()
                .0,
            crate::sem::DataSource::Unknown(_, _)
        ));
    }
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_delete_cte_reports_an_unsupported_operation() {
    let mut context = catalog(Dialect::sqlite());
    // The common AST can carry a DELETE CTE even though SQLite's parser rejects it.
    let statement = parse_statements(
        "WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d",
        Dialect::postgres(),
    )
    .next()
    .unwrap()
    .unwrap();
    assert!(
        statement
            .node
            .resolve(
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_ok()
    );
    assert!(context.reported_problems().iter().any(|problem| {
        problem.delete_problem().is_some_and(|problem| {
            problem.kind() == &crate::sem::problem::DeleteProblemKind::sqlite_cte()
        })
    }));
}
