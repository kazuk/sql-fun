mod join_and_key_semantics;
mod views;

use self::test_helpers::analyze;
use crate::ast::Dialect;
use crate::context::ParseContext;
use crate::sem::{ColumnName, PgBuiltInType};

#[rstest::rstest]
#[case::values("INSERT INTO items VALUES (1, 'one') RETURNING *")]
#[case::partial("INSERT INTO items VALUES (1) RETURNING id")]
#[case::named("INSERT INTO items(label, id) VALUES ('one', 1) RETURNING id AS n")]
#[case::defaults("INSERT INTO items DEFAULT VALUES RETURNING *")]
#[case::default_cell("INSERT INTO items VALUES (DEFAULT, 'one') RETURNING id")]
#[case::multi_row("INSERT INTO items(id) VALUES (1), (2) RETURNING id")]
#[case::select("INSERT INTO items(id) SELECT 1 RETURNING id")]
#[case::select_star("INSERT INTO items SELECT * FROM items RETURNING id")]
#[case::join_star(
    "INSERT INTO items SELECT * FROM (SELECT id FROM items) l JOIN (SELECT label FROM items) r ON true RETURNING id"
)]
#[case::using_star(
    "INSERT INTO items(id) SELECT * FROM (SELECT id FROM items) l JOIN (SELECT id FROM items) r USING(id) RETURNING id"
)]
#[case::derived_star("INSERT INTO items(id) SELECT s.* FROM (SELECT 1 AS n) s RETURNING id")]
#[case::cte_star("WITH c AS (SELECT 1 AS n) INSERT INTO items(id) SELECT * FROM c RETURNING id")]
#[case::compound("INSERT INTO items(id) SELECT 1 UNION ALL SELECT 2 RETURNING id")]
#[case::cte_source("WITH c AS (SELECT 1 AS n) INSERT INTO items(id) SELECT n FROM c RETURNING id")]
#[case::no_returning("INSERT INTO items(id) VALUES (1)")]
#[case::alias("INSERT INTO items AS i VALUES (1, 'one') RETURNING i.id")]
#[case::nothing("INSERT INTO items(id) VALUES (1) ON CONFLICT DO NOTHING RETURNING id")]
fn retains_insert_target_binding(#[case] sql: &str) {
    let (statement, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    let insert = statement.as_insert().expect("analyzed INSERT");
    assert_eq!(insert.target().to_string(), "public.items");
}

#[rstest::rstest]
#[case::cte("WITH c AS (SELECT $1 n) INSERT INTO items(id) SELECT n FROM c")]
#[case::value("INSERT INTO items(id) VALUES ($1) RETURNING id")]
#[case::select("INSERT INTO items(id) SELECT $1 RETURNING id")]
#[case::later_row("INSERT INTO items(id) VALUES (1), ($1) RETURNING id")]
#[case::repeated("INSERT INTO items(id) VALUES ($1), ($1) RETURNING id")]
#[case::compound("INSERT INTO items(id) SELECT $1 UNION ALL SELECT $1 RETURNING id")]
fn infers_assignment_parameters(#[case] sql: &str) {
    let (_, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    let constraint = context
        .parameter_constraints()
        .parameter(1)
        .expect("parameter");
    assert!(constraint.constraint().allows(&PgBuiltInType::int4()));
    assert!(!constraint.constraint().allows(&PgBuiltInType::text()));
}

#[rstest::rstest]
#[case::explicit(
    "INSERT INTO expanded_items(a, b, c) SELECT ($1).*, $2 WHERE $1::public.pair IS NOT NULL"
)]
#[case::implicit("INSERT INTO expanded_items SELECT ($1).*, $2 WHERE $1::public.pair IS NOT NULL")]
fn inferred_composite_star_assigns_expanded_insert_columns(#[case] sql: &str) {
    let dialect = Dialect::postgres();
    let mut context = test_helpers::catalog(dialect);
    let pair_name = crate::sem::FullName::with_schema("public", "pair");
    let mut fields = crate::sem::CompositeFieldCollection::default();
    fields.push(crate::sem::CompositeFieldDefinition::new(
        "a",
        &Some(PgBuiltInType::int4()),
        &None,
    ));
    fields.push(crate::sem::CompositeFieldDefinition::new(
        "b",
        &Some(PgBuiltInType::text()),
        &None,
    ));
    let pair = crate::sem::CreateCompositeType::new(&pair_name, &fields);
    context.set_get_type_impl_result(&pair_name, &crate::sem::TypeDefinition::from(&pair));
    let table_statement = crate::parse_statements(
        "CREATE TABLE expanded_items(a integer, b text, c integer, d integer GENERATED ALWAYS AS (1) STORED)",
        dialect,
    )
    .next()
    .unwrap()
    .unwrap();
    context = table_statement.analyze(context).unwrap().into_context();
    let statement = crate::parse_statements(sql, dialect)
        .next()
        .unwrap()
        .unwrap();
    let result = statement.node.resolve(&mut context).unwrap();
    assert!(result.as_insert().is_some());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    assert_eq!(
        result
            .as_insert()
            .unwrap()
            .source()
            .as_query()
            .unwrap()
            .result_columns()
            .len(),
        3
    );
    let second = context.parameter_constraints().parameter(2).unwrap();
    assert!(second.constraint().allows(&PgBuiltInType::int4()));
    assert!(!second.constraint().allows(&PgBuiltInType::text()));
}

#[test]
fn returning_is_exposed_through_insert_cte() {
    let (statement, context) = analyze(
        "WITH inserted(n) AS (INSERT INTO items VALUES ($1) RETURNING id) SELECT n FROM inserted",
        Dialect::postgres(),
    );
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    let query = statement.into_select_statemtnt().expect("SELECT");
    assert_eq!(
        query
            .result_columns()
            .get_result_column_def(&ColumnName::from("n"))
            .unwrap()
            .get_type(),
        Some(&PgBuiltInType::int4())
    );
    assert!(context.parameter_constraints().parameter(1).is_some());
}

#[test]
fn default_values_retains_analyzed_target_without_fabricated_source() {
    let (statement, _) = analyze(
        "INSERT INTO items DEFAULT VALUES RETURNING id",
        Dialect::postgres(),
    );
    let insert = statement.as_insert().unwrap();
    assert_eq!(insert.columns().len(), 2);
    assert!(insert.source().rows().is_none());
    assert!(insert.source().as_query().is_none());
}

#[test]
#[should_panic(expected = "#474")]
fn update_action_stays_explicitly_unimplemented() {
    analyze(
        "INSERT INTO items(id) VALUES (1) ON CONFLICT(id) DO UPDATE SET id = excluded.id",
        Dialect::postgres(),
    );
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::values("INSERT INTO items VALUES (1, 'one') RETURNING *")]
#[case::partial("INSERT INTO items(id) VALUES (1) RETURNING id")]
#[case::default_values("INSERT INTO items DEFAULT VALUES RETURNING id")]
#[case::select("INSERT INTO items(id) SELECT 1 RETURNING id")]
#[case::replace("INSERT OR REPLACE INTO items(id) VALUES (1) RETURNING id")]
#[case::duplicate("INSERT INTO items(id,id) VALUES(1,2) RETURNING id")]
#[case::original_qualifier("INSERT INTO items AS i(id) VALUES (1) RETURNING items.id")]
fn sqlite_insert(#[case] sql: &str) {
    let (statement, context) = analyze(sql, Dialect::sqlite());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    assert!(!statement.as_insert().unwrap().result_columns().is_empty());
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_implicit_columns_require_full_arity() {
    let (_, context) = analyze("INSERT INTO items VALUES (1)", Dialect::sqlite());
    assert!(context.reported_problem_count() > 0);
}

#[rstest::rstest]
#[case::element("INSERT INTO arrays(a[1]) VALUES($1)")]
#[case::parameter_index("INSERT INTO arrays(a[$2]) VALUES($1)")]
fn resolves_array_assignment_targets(#[case] sql: &str) {
    let (statement, context) = analyze(sql, Dialect::postgres());
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    assert_eq!(
        statement.as_insert().unwrap().columns()[0]
            .destination()
            .get_type(),
        Some(PgBuiltInType::int4())
    );
    assert!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int4())
    );
}

#[rstest::rstest]
#[case::missing_relation("INSERT INTO absent VALUES(1)", false)]
#[case::missing_column("INSERT INTO items(absent) VALUES(1)", false)]
#[case::unreadable_cte(
    "WITH changed AS (INSERT INTO items(id) VALUES(1)) SELECT 1 FROM changed",
    true
)]
fn fails_without_fabricated_bindings(#[case] sql: &str, #[case] fatal: bool) {
    let statement = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    let mut context = test_helpers::catalog(Dialect::postgres());
    let result = statement.node.resolve(&mut context);
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
fn public_postgres_entry_points_share_insert_resolution() {
    use crate::{
        PostgresDialect,
        sem::{SemAst, analyze_insert_statement},
        syn::{ListOpt, Opt},
    };
    let sql = "INSERT INTO items(id) VALUES($1) RETURNING id";
    let parsed = PostgresDialect::parse(sql).unwrap();
    let tokens = PostgresDialect::scan(sql).unwrap();
    let result = SemAst::semantic_analysis(
        test_helpers::catalog(Dialect::postgres()),
        parsed.clone(),
        &tokens,
    )
    .unwrap();
    assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
    assert!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .is_some()
    );
    let insert = parsed
        .get_stmts()
        .get(0)
        .get_stmt()
        .as_insert_stmt()
        .as_inner()
        .unwrap();
    let (result, context) =
        analyze_insert_statement(test_helpers::catalog(Dialect::postgres()), &None, insert)
            .unwrap();
    assert_eq!(result.as_insert().unwrap().result_columns().len(), 1);
    assert!(context.parameter_constraints().parameter(1).is_some());
}

#[test]
fn unreferenced_modifying_cte_is_still_analyzed_once() {
    let before = crate::sem::parameter_resolution::inference_runs();
    let (_, context) = analyze(
        "WITH changed AS (INSERT INTO items(id) VALUES($1)) SELECT 1",
        Dialect::postgres(),
    );
    assert_eq!(context.reported_problem_count(), 0);
    assert!(context.parameter_constraints().parameter(1).is_some());
    assert_eq!(
        crate::sem::parameter_resolution::inference_runs(),
        before + 1
    );
}

#[rstest::rstest]
#[case::value("INSERT INTO items(id) VALUES($1)")]
#[case::subscript("INSERT INTO arrays(a[$1]) VALUES(1)")]
fn assignment_casts_do_not_become_overload_implicit_casts(#[case] sql: &str) {
    use crate::sem::{CastContext, CastDefinition};
    let mut context = test_helpers::catalog(Dialect::postgres());
    context.setup_type(PgBuiltInType::int8());
    context.set_get_implicit_cast_result(
        &PgBuiltInType::int8(),
        &PgBuiltInType::int4(),
        Some(CastDefinition::new(CastContext::assignment())),
    );
    let statement = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap();
    statement.node.resolve(&mut context).unwrap();
    assert!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int8())
    );
    assert!(
        !crate::context::CastInfoRead::get_implicit_cast(
            &context,
            &PgBuiltInType::int8(),
            &PgBuiltInType::int4()
        )
        .unwrap()
        .use_in_implicit()
    );
    assert_eq!(context.reported_problem_count(), 0);
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::values("INSERT INTO items VALUES(1,'one') RETURNING *", true)]
#[case::short_values("INSERT INTO items VALUES(1)", false)]
#[case::explicit_column("INSERT INTO items(id) VALUES(1) RETURNING id", true)]
#[case::duplicates("INSERT INTO items(id,id) VALUES(1,2)", true)]
#[case::qualified_original("INSERT INTO items AS i(id) VALUES(1) RETURNING items.id", true)]
#[case::qualified_alias("INSERT INTO items AS i(id) VALUES(1) RETURNING i.id", false)]
#[case::qualified_star("INSERT INTO items(id) VALUES(1) RETURNING items.*", false)]
#[case::affinity("INSERT INTO items(id) VALUES('one') RETURNING id", true)]
fn agrees_with_bundled_sqlite(#[case] sql: &str, #[case] accepted: bool) {
    let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
    oracle
        .execute_batch("CREATE TABLE items(id INTEGER, label TEXT)")
        .unwrap();
    assert_eq!(
        oracle.prepare(sql).is_ok(),
        accepted,
        "SQLite {}: {sql}",
        ::rusqlite::version()
    );
    let (_, context) = analyze(sql, Dialect::sqlite());
    assert_eq!(
        context.reported_problem_count() == 0,
        accepted,
        "{:#?}",
        context.reported_problems()
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn public_sqlite_entry_point_retains_returning_and_parameters() {
    use crate::{sem::SemAst, sqlite::SqliteDialect};
    let sql = "INSERT INTO items(id) VALUES(?1) RETURNING id";
    let statement = SqliteDialect::parse(sql)
        .unwrap()
        .unwrap()
        .into_plain_statement()
        .unwrap();
    let result = SemAst::semantic_analysis_sqlite_stmt(
        test_helpers::catalog(Dialect::sqlite()),
        statement,
        &SqliteDialect::scan(sql).unwrap(),
    )
    .unwrap();
    assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
    assert!(
        result
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .is_any()
    );
}

#[rstest::rstest]
#[case::unique("CREATE UNIQUE INDEX ix ON items(id)", "(id)", true)]
#[case::duplicate("CREATE UNIQUE INDEX ix ON items(id)", "(id,id)", true)]
#[case::nonunique("CREATE INDEX ix ON items(id)", "(id)", false)]
#[case::wrong_column("CREATE UNIQUE INDEX ix ON items(label)", "(id)", false)]
#[case::expression(
    "CREATE UNIQUE INDEX ix ON items((id IS NOT NULL))",
    "((id IS NOT NULL))",
    true
)]
#[case::expression_mismatch(
    "CREATE UNIQUE INDEX ix ON items((id IS NOT NULL))",
    "((id IS NULL))",
    false
)]
#[case::partial(
    "CREATE UNIQUE INDEX ix ON items(id) WHERE id IS NOT NULL",
    "(id) WHERE id IS NOT NULL",
    true
)]
#[case::partial_without_predicate(
    "CREATE UNIQUE INDEX ix ON items(id) WHERE id IS NOT NULL",
    "(id)",
    false
)]
fn validates_conflict_index(#[case] index: &str, #[case] target: &str, #[case] accepted: bool) {
    let dialect = Dialect::postgres();
    let mut context = crate::parse_statements(index, dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(test_helpers::catalog(dialect))
        .unwrap()
        .into_context();
    assert_eq!(context.reported_problem_count(), 0);
    let sql = format!("INSERT INTO items(id) VALUES(1) ON CONFLICT {target} DO NOTHING");
    let statement = crate::parse_statements(&sql, dialect)
        .next()
        .unwrap()
        .unwrap();
    let result = statement.node.resolve(&mut context).unwrap();
    assert_eq!(
        context.reported_problem_count() == 0,
        accepted,
        "{:#?}",
        context.reported_problems()
    );
    assert_eq!(result.as_insert().unwrap().conflicts().len(), 1);
    let lookup = result.as_insert().unwrap().conflicts()[0].arbiter();
    assert_eq!(lookup.is_not_found(), !accepted);
    if accepted {
        let index = lookup.arbiters().unwrap()[0].index_name().unwrap();
        assert_eq!(index, &crate::sem::FullName::with_schema("public", "ix"));
    }
}

#[test]
fn returning_retains_expression_alias_and_nullability() {
    let (statement, context) = analyze(
        "INSERT INTO items AS i VALUES(1,'x') RETURNING i.label AS name, i.id",
        Dialect::postgres(),
    );
    assert_eq!(context.reported_problem_count(), 0);
    let columns = statement.as_insert().unwrap().result_columns();
    let label = columns.get_column_at(0).unwrap();
    assert_eq!(label.column_name(), &Some(ColumnName::from("name")));
    assert_eq!(label.col_type(), &Some(PgBuiltInType::text()));
    assert_eq!(label.is_not_null(), &Some(false));
    assert!(label.expression().is_some());
    assert_eq!(columns.get_column_at(1).unwrap().is_not_null(), &Some(true));
}

#[cfg(feature = "sqlite")]
#[test]
#[should_panic(expected = "#474")]
fn sqlite_later_update_action_remains_explicitly_unimplemented() {
    analyze(
        "INSERT INTO items VALUES(1,'x') ON CONFLICT(id) DO NOTHING ON CONFLICT DO UPDATE SET label='x'",
        Dialect::sqlite(),
    );
}

#[rstest::rstest]
#[case::collation(r#"label COLLATE "C""#, false, "C")]
#[case::expression_collation(r#"(label COLLATE "C")"#, false, "C")]
#[case::qualified_collation("label COLLATE custom.casefold", false, "custom.casefold")]
#[case::operator_class("id int4_ops", true, "int4_ops")]
#[case::qualified_operator_class("id custom.integer_ops", true, "custom.integer_ops")]
fn binds_conflict_modifier_through_context(
    #[case] target: &str,
    #[case] operator: bool,
    #[case] requested: &str,
    #[values("found", "missing", "ambiguous")] outcome: &str,
) {
    let (statement, context, expected) =
        test_helpers::modifier_analysis(target, operator, requested, outcome, Dialect::postgres());
    let conflict = &statement.as_insert().unwrap().conflicts()[0];
    let modifier = &conflict.modifiers()[0];
    let lookup = if operator {
        modifier.operator_class()
    } else {
        modifier.collation()
    }
    .unwrap();
    assert_eq!(lookup, &expected);
    assert_eq!(
        lookup.requested(),
        &requested
            .parse::<crate::sem::FullName>()
            .expect("logical fixture name")
    );
    if outcome == "found" {
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        assert_eq!(
            conflict.arbiter().arbiters().unwrap()[0]
                .index_name()
                .unwrap()
                .to_string(),
            "public.mock_arbiter"
        );
        assert!(lookup.resolved_name().unwrap().schema().is_some());
    } else {
        assert!(conflict.arbiter().is_not_found());
        let kind = context.reported_problems()[0]
            .insert_problem()
            .unwrap()
            .kind();
        if outcome == "missing" {
            assert!(lookup.is_missing());
            assert_eq!(
                kind,
                crate::sem::problem::InsertProblemKind::ConflictNameMissing
            );
        } else {
            assert_eq!(lookup.ambiguous_candidates().unwrap().len(), 2);
            assert_eq!(
                kind,
                crate::sem::problem::InsertProblemKind::ConflictNameAmbiguous
            );
        }
    }
    let json = ::serde_json::to_string(statement.as_insert().unwrap()).unwrap();
    let roundtrip: crate::sem::InsertStatement = ::serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.conflicts()[0].modifiers(), conflict.modifiers());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::lower("label COLLATE nocase", "nocase")]
#[case::parenthesized("(label COLLATE nocase)", "nocase")]
#[case::mixed_case("label COLLATE NoCaSe", "NoCaSe")]
fn sqlite_conflict_collation_uses_connection_catalog(
    #[case] target: &str,
    #[case] requested: &str,
    #[values("found", "missing", "ambiguous")] outcome: &str,
) {
    let (statement, context, expected) =
        test_helpers::modifier_analysis(target, false, requested, outcome, Dialect::sqlite());
    let conflict = &statement.as_insert().unwrap().conflicts()[0];
    assert_eq!(conflict.modifiers()[0].collation(), Some(&expected));
    assert!(conflict.modifiers()[0].operator_class().is_none());
    if outcome == "found" {
        assert_eq!(context.reported_problem_count(), 0);
        assert!(expected.resolved_name().unwrap().schema().is_none());
    } else {
        assert!(conflict.arbiter().is_not_found());
        assert!(context.reported_problem_count() > 0);
    }
}

#[test]
fn preserves_modifier_positions_and_looks_up_both_names() {
    use crate::sem::{CatalogNameLookup, FullName, InsertArbiter, InsertInferenceModifiers};
    let dialect = Dialect::postgres();
    let mut context = test_helpers::catalog(dialect);
    let collation_name = FullName::from_local_name("C");
    let class_name = FullName::from_local_name("text_ops");
    let collation =
        CatalogNameLookup::found(&collation_name, &FullName::with_schema("pg_catalog", "C"));
    let operator_class = CatalogNameLookup::found(
        &class_name,
        &FullName::with_schema("pg_catalog", "text_ops"),
    );
    context.set_collation_lookup(&collation_name, &collation);
    context.set_operator_class_lookup(&class_name, &operator_class);
    let expected = vec![
        InsertInferenceModifiers::new(None, None),
        InsertInferenceModifiers::new(Some(collation), Some(operator_class)),
    ];
    context.expect_conflict_modifiers(
        &expected,
        &[InsertArbiter::index(&FullName::with_schema(
            "public", "both",
        ))],
    );
    let result = crate::parse_statements(
        r#"INSERT INTO items VALUES(1,'x') ON CONFLICT(id, label COLLATE "C" text_ops) DO NOTHING"#,
        dialect,
    )
    .next()
    .unwrap()
    .unwrap()
    .node
    .resolve(&mut context)
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    assert_eq!(
        result.as_insert().unwrap().conflicts()[0].modifiers(),
        expected
    );
}

#[test]
fn retains_failed_inner_collation_even_when_outer_collation_is_bound() {
    use crate::sem::{CatalogNameLookup, FullName};
    let dialect = Dialect::postgres();
    let mut context = test_helpers::catalog(dialect);
    let missing = FullName::from_local_name("absent579");
    let outer = FullName::from_local_name("C");
    let missing_lookup = CatalogNameLookup::missing(&missing);
    let outer_lookup = CatalogNameLookup::found(&outer, &FullName::with_schema("pg_catalog", "C"));
    context.set_collation_lookup(&missing, &missing_lookup);
    context.set_collation_lookup(&outer, &outer_lookup);
    // Any attempt to infer an arbiter from these failed bindings is a bug.
    context.expect_conflict_modifiers(&[], &[]);
    let result = crate::parse_statements(
        r#"INSERT INTO items VALUES(1,'x') ON CONFLICT ((label COLLATE absent579) COLLATE "C") DO NOTHING"#,
        dialect,
    ).next().unwrap().unwrap().node.resolve(&mut context).unwrap();
    let conflict = &result.as_insert().unwrap().conflicts()[0];
    assert_eq!(
        conflict.modifiers()[0].collation_references(),
        &[missing_lookup, outer_lookup.clone()]
    );
    assert_eq!(conflict.modifiers()[0].collation(), Some(&outer_lookup));
    assert!(conflict.arbiter().is_not_found());
    assert_eq!(
        context.reported_problems()[0]
            .insert_problem()
            .unwrap()
            .kind(),
        crate::sem::problem::InsertProblemKind::ConflictNameMissing
    );
}

#[test]
fn resolved_modifier_does_not_guarantee_an_eligible_arbiter() {
    let (statement, context, lookup) = test_helpers::modifier_analysis(
        "id int4_ops",
        true,
        "int4_ops",
        "unmatched",
        Dialect::postgres(),
    );
    let conflict = &statement.as_insert().unwrap().conflicts()[0];
    assert!(lookup.resolved_name().is_some());
    assert_eq!(conflict.modifiers()[0].operator_class(), Some(&lookup));
    assert!(conflict.arbiter().is_not_found());
    assert_eq!(
        context.reported_problems()[0]
            .insert_problem()
            .unwrap()
            .kind(),
        crate::sem::problem::InsertProblemKind::ConflictTarget
    );
}

#[rstest::rstest]
#[case::primary("PRIMARY KEY", crate::sem::InsertConstraintKind::PrimaryKey)]
#[case::unique("UNIQUE", crate::sem::InsertConstraintKind::Unique)]
fn resolves_named_constraint_identity(
    #[case] kind: &str,
    #[case] expected: crate::sem::InsertConstraintKind,
) {
    let dialect = Dialect::postgres();
    let ddl = format!("CREATE TABLE keyed(id int4, CONSTRAINT key473 {kind}(id))");
    let mut context = crate::parse_statements(&ddl, dialect)
        .next()
        .unwrap()
        .unwrap()
        .analyze(test_helpers::catalog(dialect))
        .unwrap()
        .into_context();
    let statement = crate::parse_statements(
        "INSERT INTO keyed VALUES(1) ON CONFLICT ON CONSTRAINT key473 DO NOTHING",
        dialect,
    )
    .next()
    .unwrap()
    .unwrap();
    let result = statement.node.resolve(&mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    let lookup = result.as_insert().unwrap().conflicts()[0]
        .arbiter()
        .constraint()
        .unwrap();
    assert_eq!(lookup.kind(), expected);
    assert_eq!(lookup.name(), "key473");
    assert_eq!(lookup.table().to_string(), "public.keyed");
    let returning = crate::parse_statements("INSERT INTO keyed VALUES(2) RETURNING id", dialect)
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context)
        .unwrap();
    let nullability = returning
        .as_insert()
        .unwrap()
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .is_not_null();
    if kind == "PRIMARY KEY" {
        assert_eq!(nullability, &Some(true));
    }
}

#[test]
fn retains_failed_constraint_lookup_and_structured_problem() {
    let (statement, context) = analyze(
        "INSERT INTO items VALUES(1,'x') ON CONFLICT ON CONSTRAINT absent DO NOTHING",
        Dialect::postgres(),
    );
    let lookup = statement.as_insert().unwrap().conflicts()[0]
        .arbiter()
        .constraint()
        .unwrap();
    assert_eq!(lookup.kind(), crate::sem::InsertConstraintKind::NotFound);
    assert_eq!(lookup.name(), "absent");
    assert_eq!(
        context.reported_problems()[0]
            .insert_problem()
            .unwrap()
            .kind(),
        crate::sem::problem::InsertProblemKind::ConflictConstraint
    );
}

#[test]
fn retains_multiple_unnamed_table_keys_without_fabricating_names() {
    let dialect = Dialect::postgres();
    let mut context = crate::parse_statements(
        "CREATE TABLE keyed(id int4, label text, UNIQUE(id), UNIQUE(label))",
        dialect,
    )
    .next()
    .unwrap()
    .unwrap()
    .analyze(test_helpers::catalog(dialect))
    .unwrap()
    .into_context();
    let statement = crate::parse_statements(
        "INSERT INTO keyed VALUES(1,'x') ON CONFLICT(label) DO NOTHING",
        dialect,
    )
    .next()
    .unwrap()
    .unwrap();
    let result = statement.node.resolve(&mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:#?}",
        context.reported_problems()
    );
    let arbiters = result.as_insert().unwrap().conflicts()[0]
        .arbiter()
        .arbiters()
        .unwrap();
    assert_eq!(
        arbiters[0].anonymous_key_columns(),
        Some([ColumnName::from("label")].as_slice())
    );
    assert!(arbiters[0].constraint().is_none());
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::test_helpers::{TestParseContext, source_span};
    use crate::{
        context::MutSourcePathWithSpan,
        sem::{ColumnDefinition, ColumnName, PgBuiltInType, SchemaName, SemAst},
    };

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
                    Some(true),
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
        let array =
            crate::sem::TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
        context.setup_type(array.clone());
        context.setup_table(
            "arrays",
            &[ColumnDefinition::new(
                &Some(ColumnName::from("a")),
                Some(&array),
                Some(false),
                &[],
            )],
            &source_span(),
        );
        let identity = crate::sem::IdentityColumnConstraint::new(
            &crate::sem::TableName::from(crate::sem::FullName::with_schema(
                "public",
                "identity_items",
            )),
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
            &source_span(),
        );
        context
    }
    pub(super) fn modifier_analysis(
        target: &str,
        operator: bool,
        requested: &str,
        outcome: &str,
        dialect: Dialect,
    ) -> (SemAst, TestParseContext, crate::sem::CatalogNameLookup) {
        use crate::sem::{CatalogNameLookup, FullName, InsertArbiter, InsertInferenceModifiers};
        let mut context = catalog(dialect);
        let requested_name = if let Some((schema, local)) = requested.split_once('.') {
            FullName::with_schema(schema, local)
        } else {
            FullName::from_local_name(requested)
        };
        let canonical = if dialect == Dialect::sqlite() {
            FullName::from_local_name("NOCASE")
        } else if requested_name.schema().is_some() {
            requested_name.clone()
        } else {
            FullName::with_schema("pg_catalog", requested)
        };
        let lookup = match outcome {
            "found" | "unmatched" => CatalogNameLookup::found(&requested_name, &canonical),
            "missing" => CatalogNameLookup::missing(&requested_name),
            "ambiguous" => CatalogNameLookup::ambiguous(
                &requested_name,
                &[canonical, FullName::with_schema("custom", "other")],
            ),
            _ => panic!("unknown test outcome"),
        };
        let modifier = if operator {
            context.set_operator_class_lookup(&requested_name, &lookup);
            InsertInferenceModifiers::new(None, Some(lookup.clone()))
        } else {
            context.set_collation_lookup(&requested_name, &lookup);
            InsertInferenceModifiers::new(Some(lookup.clone()), None)
        };
        // The mock independently checks the bound identities passed to inference.
        // Catalog eligibility is deliberately not implemented by the resolver.
        let arbiters = if outcome == "unmatched" {
            Vec::new()
        } else {
            vec![InsertArbiter::index(&FullName::with_schema(
                "public",
                "mock_arbiter",
            ))]
        };
        context.expect_conflict_modifiers(&[modifier], &arbiters);
        let sql = format!("INSERT INTO items VALUES(1,'x') ON CONFLICT({target}) DO NOTHING");
        let statement = crate::parse_statements(&sql, dialect)
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve(&mut context)
            .unwrap();
        (statement, context, lookup)
    }
    pub(super) fn analyze(sql: &str, dialect: Dialect) -> (SemAst, TestParseContext) {
        let statement = crate::parse_statements(sql, dialect)
            .next()
            .unwrap()
            .unwrap();
        let mut context = catalog(dialect);
        let result = statement
            .node
            .resolve(&mut context)
            .expect("semantic analysis");
        (result, context)
    }
}

#[test]
fn nested_insert_cte_reports_a_query_problem() {
    let mut context = test_helpers::catalog(Dialect::postgres());
    let statement = crate::parse_statements("SELECT * FROM (WITH changed AS (INSERT INTO items(id) VALUES(1) RETURNING id) SELECT id FROM changed) s", Dialect::postgres()).next().unwrap().unwrap();
    assert!(statement.node.resolve(&mut context).is_ok());
    assert!(context.reported_problem_count() > 0);
}
