//! Maintained SQL-to-catalog column-rename transitions.

#[cfg(feature = "sqlite")]
use crate::names::FullName as NamesFullName;
use crate::{
    context::{GetSourcePathWithSpan, ParseContext, TableInfoRead, ViewInfoRead},
    names::{ColumnName, SchemaName, TableName},
    sem::{ColumnConstraint, IndirectionExpr, SemScalarExpr},
    test_helpers::{
        column_rename::{child as column_rename_child, name, schema},
        ownership::apply_ownership_sql,
    },
};
#[rstest::rstest]
#[case::ordinary("ALTER TABLE t RENAME COLUMN a TO c")]
#[case::optional_keyword("ALTER TABLE t RENAME a TO c")]
#[case::qualified("ALTER TABLE public.t RENAME COLUMN a TO c")]
#[case::only("ALTER TABLE ONLY t RENAME COLUMN a TO c")]
fn rename_preserves_attributes_order_history_and_parent(
    #[case] sql: &str,
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = schema(&[
        "CREATE TABLE t(a integer NOT NULL DEFAULT 1, b integer)",
        "ALTER TABLE t ALTER a SET STATISTICS 20",
    ])?;
    let parent = if inherited {
        let parent = context.freeze();
        context = column_rename_child(&parent);
        Some(parent)
    } else {
        None
    };
    let before = context
        .get_table_impl(&name("t"))
        .unwrap()
        .get_column_at(0)
        .unwrap()
        .clone();
    let source = context
        .get_table_impl(&name("t"))
        .unwrap()
        .get_source_path_with_span()
        .clone();
    let result = apply_ownership_sql(context, sql)?;
    assert!(result.context().problems().is_empty());
    let change = result.ast().alter_table_statement().unwrap();
    assert!(change.can_apply());
    assert_eq!(change.resolved_target().unwrap().full_name(), &name("t"));
    assert_eq!(
        change.column_rename_change().unwrap().source(),
        Some(&before)
    );
    let table = result.context().get_table_impl(&name("t")).unwrap();
    assert!(!table.has_column(&ColumnName::from("a")));
    let column = table.get_column_at(0).unwrap();
    assert_eq!(column.name().as_ref().unwrap().as_str(), "c");
    assert_eq!(column.get_type(), before.get_type());
    assert_eq!(column.is_not_null(), before.is_not_null());
    assert_eq!(column.statistics_target(), Some(20));
    assert_eq!(
        table
            .get_column_at(1)
            .unwrap()
            .name()
            .as_ref()
            .unwrap()
            .as_str(),
        "b"
    );
    assert_eq!(table.alter_statements().len(), 2);
    assert_eq!(table.get_source_path_with_span().span(), source.span());
    if let Some(parent) = parent {
        assert!(
            parent
                .get_table_impl(&name("t"))
                .unwrap()
                .has_column(&ColumnName::from("a"))
        );
    }
    let result = apply_ownership_sql(result.into_context(), "SELECT c FROM t")?;
    assert!(result.context().problems().is_empty());
    Ok(())
}
#[rstest::rstest]
#[case::same("ALTER TABLE t RENAME a TO a", 1)]
#[case::duplicate("ALTER TABLE t RENAME a TO b", 1)]
#[case::missing_column("ALTER TABLE t RENAME missing TO c", 1)]
#[case::missing_relation("ALTER TABLE missing RENAME a TO c", 1)]
#[case::skip("ALTER TABLE IF EXISTS missing RENAME a TO c", 0)]
#[case::wrong_kind("ALTER TABLE IF EXISTS v RENAME a TO c", 1)]
fn rejection_and_skip_are_atomic(
    #[case] sql: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    let context = schema(&[
        "CREATE TABLE t(a integer, b integer)",
        "CREATE VIEW v AS SELECT a FROM t",
    ])?;
    let result = apply_ownership_sql(context, sql)?;
    assert!(!result.ast().alter_table_statement().unwrap().can_apply());
    assert_eq!(result.context().problems().len(), count);
    let table = result.context().get_table_impl(&name("t")).unwrap();
    assert!(table.has_column(&ColumnName::from("a")));
    assert_eq!(table.alter_statements().len(), 0);
    let result = apply_ownership_sql(result.into_context(), "SELECT a FROM t")?;
    assert_eq!(result.context().problems().len(), count);
    Ok(())
}
#[test]
fn repeated_renames_old_name_reuse_and_quoted_names() -> testresult::TestResult {
    let context = schema(&[
        r#"CREATE TABLE t("A" integer, a integer)"#,
        r#"ALTER TABLE t RENAME "A" TO "B""#,
        r#"ALTER TABLE t RENAME a TO "A""#,
        r#"ALTER TABLE t RENAME "B" TO c"#,
    ])?;
    let table = context.get_table_impl(&name("t")).unwrap();
    assert!(table.has_column(&ColumnName::from("c")));
    assert!(table.has_column(&ColumnName::from("A")));
    assert!(!table.has_column(&ColumnName::from("a")));
    assert_eq!(table.alter_statements().len(), 3);
    Ok(())
}
#[test]
fn view_output_is_fixed_but_binding_and_lineage_follow_column() -> testresult::TestResult {
    let context = schema(&[
        "CREATE TABLE t(a integer)",
        "CREATE VIEW v AS SELECT a, a AS alias FROM t",
        "ALTER TABLE t RENAME a TO c",
    ])?;
    let view = context.get_view_impl(&name("v")).unwrap();
    assert_eq!(
        view.result_columns().names(),
        vec![ColumnName::from("a"), ColumnName::from("alias")]
    );
    assert_eq!(
        view.ordinary_definition()
            .unwrap()
            .column_lineage()
            .source_column(0),
        Some(&ColumnName::from("c"))
    );
    let result = apply_ownership_sql(context, "SELECT a, alias FROM v")?;
    assert!(result.context().problems().is_empty());
    Ok(())
}
#[rstest::rstest]
fn unqualified_foreign_keys_keep_creation_time_identity(
    #[values(false, true)] inherited: bool,
) -> testresult::TestResult {
    let mut context = schema(&[
        "CREATE TABLE a.parent(id integer PRIMARY KEY)",
        "CREATE TABLE b.parent(id integer PRIMARY KEY)",
    ])?;
    context.set_search_path(&[
        SchemaName::from("a"),
        SchemaName::from("b"),
        SchemaName::from("public"),
    ]);
    let context = apply_ownership_sql(
        context,
        "CREATE TABLE public.child(id integer REFERENCES parent(id))",
    )?
    .into_context();
    let mut context = apply_ownership_sql(
        context,
        "ALTER TABLE public.child ADD CONSTRAINT fk FOREIGN KEY (id) REFERENCES parent(id)",
    )?
    .into_context();
    let parent = if inherited {
        let parent = context.freeze();
        context = column_rename_child(&parent);
        Some(parent)
    } else {
        None
    };
    context.set_search_path(&[
        SchemaName::from("b"),
        SchemaName::from("a"),
        SchemaName::from("public"),
    ]);
    let context =
        apply_ownership_sql(context, "ALTER TABLE parent RENAME id TO unrelated")?.into_context();
    let context =
        apply_ownership_sql(context, "ALTER TABLE a.parent RENAME id TO renamed")?.into_context();
    assert!(context.problems().is_empty());
    let child = context.get_table_impl(&name("child")).unwrap();
    let fk = child
        .get_column_at(0)
        .unwrap()
        .constraints()
        .iter()
        .find_map(|constraint| constraint.as_foreign_key())
        .unwrap();
    assert_eq!(fk.primary_key_column().as_str(), "renamed");
    if let Some(parent) = &parent {
        let original = parent
            .get_table_impl(&name("child"))
            .unwrap()
            .get_column_at(0)
            .unwrap()
            .constraints()
            .iter()
            .find_map(ColumnConstraint::as_foreign_key)
            .unwrap();
        assert_eq!(original.primary_key_column().as_str(), "id");
    }

    let table_fk = child.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(
        table_fk.target_columns(),
        Some([ColumnName::from("renamed")].as_slice())
    );
    assert_eq!(
        fk.primary_key_table()
            .unwrap()
            .full_name()
            .schema()
            .unwrap()
            .as_str(),
        "a"
    );
    Ok(())
}
#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::ordinary("ALTER TABLE t RENAME a TO c", "c")]
#[case::case_insensitive("ALTER TABLE T RENAME A TO c", "c")]
#[case::same("ALTER TABLE t RENAME a TO a", "a")]
fn sqlite_view_outputs_follow_unaliased_names(
    #[case] sql: &str,
    #[case] output: &str,
) -> testresult::TestResult {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let context = sqlite_schema(&[
        "CREATE TABLE t(a)",
        "CREATE VIEW v AS SELECT a, a AS fixed FROM t",
    ])?;
    let result = sqlite(context, sql)?;
    assert!(result.ast().alter_table_statement().unwrap().can_apply());
    assert!(result.context().problems().is_empty());
    let view = result
        .context()
        .get_view_impl(&NamesFullName::with_schema("main", "v"))
        .unwrap();
    assert_eq!(
        view.result_columns().names(),
        vec![ColumnName::from(output), ColumnName::from("fixed")]
    );
    Ok(())
}
#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::ambiguous(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS SELECT a FROM t JOIN u ON 1",
    true
)]
#[case::qualified(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS SELECT t.a FROM t JOIN u ON 1",
    false
)]
#[case::cte(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS WITH q AS (SELECT a FROM t) SELECT a FROM q",
    true
)]
#[case::explicit_cte(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS WITH q(x) AS (SELECT a FROM t) SELECT x FROM q",
    false
)]
#[case::using(
    "CREATE TABLE u(a)",
    "CREATE VIEW v AS SELECT t.a FROM t JOIN u USING(a)",
    true
)]
#[case::unrelated_using(
    "CREATE TABLE u(a)",
    "CREATE VIEW v AS SELECT x.a FROM u AS x JOIN u AS y USING(a)",
    false
)]
#[case::subquery(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS SELECT a FROM (SELECT a FROM t)",
    true
)]
fn sqlite_checks_rewritten_namespaces_before_publication(
    #[case] other: &str,
    #[case] view: &str,
    #[case] rejected: bool,
) -> testresult::TestResult {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let oracle = rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch("CREATE TABLE t(a)")?;
    oracle.execute_batch(other)?;
    oracle.execute_batch(view)?;
    assert_eq!(
        oracle.execute_batch("ALTER TABLE t RENAME a TO c").is_err(),
        rejected
    );
    let context = sqlite_schema(&["CREATE TABLE t(a)", other, view])?;
    let result = sqlite(context, "ALTER TABLE t RENAME a TO c")?;
    assert_eq!(
        !result.ast().alter_table_statement().unwrap().can_apply(),
        rejected
    );
    let table = result
        .context()
        .get_table_impl(&NamesFullName::with_schema("main", "t"))
        .unwrap();
    assert_eq!(table.has_column(&ColumnName::from("a")), rejected);
    Ok(())
}

#[test]
fn table_keys_indexes_and_fk_source_columns_follow_the_rename() -> testresult::TestResult {
    use crate::context::IndexInfoRead;
    let context = schema(&[
        "CREATE TABLE parent(id integer PRIMARY KEY)",
        "CREATE TABLE t(a integer, b integer)",
        "ALTER TABLE t ADD CONSTRAINT pk PRIMARY KEY(a)",
        "ALTER TABLE t ADD CONSTRAINT fk FOREIGN KEY(a) REFERENCES parent(id)",
        "CREATE UNIQUE INDEX idx ON t(a) INCLUDE(b)",
        "ALTER TABLE t RENAME a TO c",
        "ALTER TABLE t RENAME b TO included",
    ])?;
    let table = context.get_table_impl(&name("t")).unwrap();
    assert_eq!(table.primary_key_columns(), vec![&ColumnName::from("c")]);
    let fk = table.constraint("fk").unwrap().as_foreign_key().unwrap();
    assert_eq!(fk.source_columns(), &[ColumnName::from("c")]);
    assert_eq!(
        fk.target_columns(),
        Some([ColumnName::from("id")].as_slice())
    );
    let indexes = context.indexes_for_table(&TableName::from(name("t")));
    assert_eq!(indexes.len(), 2);
    assert!(
        indexes
            .iter()
            .all(|index| index.name().name().as_str() == "pk"
                || index.name().name().as_str() == "idx")
    );
    let result = apply_ownership_sql(
        context,
        "INSERT INTO t(c, included) VALUES (1, 2) ON CONFLICT(c) DO NOTHING",
    )?;
    assert!(result.context().problems().is_empty());
    Ok(())
}
#[test]
fn row_type_projections_and_explicit_column_aliases_remain_usable() -> testresult::TestResult {
    let context = schema(&[
        "CREATE TABLE t(a integer)",
        "CREATE TABLE holder(value public.t)",
        "CREATE VIEW v AS SELECT q.a FROM t AS q(a)",
        "CREATE VIEW row_v AS SELECT (value).a AS field FROM holder",
        "CREATE VIEW anonymous_v AS SELECT (ROW(1)).f1 AS value",
        "CREATE VIEW whole_v AS SELECT (q.*) IS NULL AS empty FROM t AS q(a)",
        "CREATE VIEW whole_field_v AS SELECT (q.*).a FROM t AS q(a)",
        "ALTER TABLE t RENAME a TO c",
    ])?;
    let whole = context
        .get_view_impl(&name("whole_v"))
        .unwrap()
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(
        crate::test_helpers::column_rename::whole_row_columns(whole, &context),
        vec![vec![ColumnName::from("a")]]
    );
    let expression = context
        .get_view_impl(&name("whole_field_v"))
        .unwrap()
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let SemScalarExpr::Indirection(IndirectionExpr::MemberRef(member)) = expression else {
        panic!("retained row field");
    };
    assert_eq!(member.member(), "a");
    let result = apply_ownership_sql(context, "SELECT (value).c FROM holder")?;
    assert!(result.context().problems().is_empty());
    assert_eq!(
        result
            .context()
            .get_view_impl(&name("v"))
            .unwrap()
            .result_columns()
            .names(),
        vec![ColumnName::from("a")]
    );
    Ok(())
}
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_rejects_a_broken_dependent_view_atomically() -> testresult::TestResult {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let context = sqlite_schema(&[
        "CREATE TABLE t(a)",
        "CREATE VIEW v1 AS SELECT a FROM t",
        "CREATE VIEW v2 AS SELECT a FROM v1",
    ])?;
    let oracle = rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch(
        "CREATE TABLE t(a); CREATE VIEW v1 AS SELECT a FROM t; CREATE VIEW v2 AS SELECT a FROM v1",
    )?;
    assert!(oracle.execute_batch("ALTER TABLE t RENAME a TO c").is_err());
    let result = sqlite(context, "ALTER TABLE t RENAME a TO c")?;
    assert!(!result.ast().alter_table_statement().unwrap().can_apply());
    let view = result
        .context()
        .get_view_impl(&NamesFullName::with_schema("main", "v1"))
        .unwrap();
    assert_eq!(view.result_columns().names(), vec![ColumnName::from("a")]);
    Ok(())
}

#[test]
fn triggers_and_rules_follow_bindings_without_changing_names_or_parent() -> testresult::TestResult {
    use crate::{
        context::{RuleInfoRead, TriggerInfoRead},
        sem::{RuleIdentity, TriggerIdentity},
        test_helpers::column_rename::{bound_columns, child},
    };
    let context = schema(&[
        "CREATE TABLE t(a integer)",
        "CREATE FUNCTION audit_function() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'",
        "CREATE TRIGGER audit BEFORE UPDATE OF a ON t FOR EACH ROW WHEN (NEW.a IS NULL) EXECUTE FUNCTION audit_function()",
        "CREATE RULE audit_rule AS ON UPDATE TO t DO ALSO SELECT NEW.a",
    ])?;
    let parent = context.freeze();
    let result = apply_ownership_sql(child(&parent), "ALTER TABLE t RENAME a TO c")?;
    assert!(result.context().problems().is_empty());
    let identity = TriggerIdentity::postgres(&name("t"), &name("audit"));
    let trigger = result.context().trigger(&identity).unwrap();
    assert_eq!(trigger.events()[0].columns()[0].name().as_str(), "c");
    assert_eq!(bound_columns(trigger.predicate().unwrap()), vec!["c"]);
    assert_eq!(
        parent.trigger(&identity).unwrap().events()[0].columns()[0]
            .name()
            .as_str(),
        "a"
    );
    let rule = result
        .context()
        .rule(&RuleIdentity::new(&name("t"), "audit_rule"))
        .unwrap();
    let expression = rule.actions()[0]
        .result_columns()
        .unwrap()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(bound_columns(expression), vec!["c"]);
    Ok(())
}
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_trigger_event_and_body_bindings_follow_rename() -> testresult::TestResult {
    use crate::{
        context::TriggerInfoRead,
        names::FullName,
        sem::{SemAst, TriggerIdentity},
        test_helpers::column_rename::{bound_columns, sqlite, sqlite_schema},
    };
    let statements = [
        "CREATE TABLE t(a)",
        "CREATE TRIGGER audit AFTER UPDATE OF a ON t BEGIN UPDATE t SET a = NEW.a; END",
    ];
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    for sql in statements {
        oracle.execute_batch(sql)?;
    }
    oracle.execute_batch("ALTER TABLE t RENAME a TO c")?;
    let result = sqlite(sqlite_schema(&statements)?, "ALTER TABLE t RENAME a TO c")?;
    assert!(result.context().problems().is_empty());
    let trigger = result
        .context()
        .trigger(&TriggerIdentity::sqlite(&FullName::with_schema(
            "main", "audit",
        )))
        .unwrap();
    assert_eq!(trigger.events()[0].columns()[0].name().as_str(), "c");
    let SemAst::UpdateStatement(update) = &trigger.body().statements()[0] else {
        panic!("UPDATE body");
    };
    assert_eq!(
        update.assignments()[0]
            .column()
            .name()
            .as_ref()
            .unwrap()
            .as_str(),
        "c"
    );
    assert_eq!(
        bound_columns(update.assignments()[0].destination()),
        vec!["c"]
    );
    Ok(())
}
#[test]
fn an_unresolved_foreign_key_is_not_bound_by_a_later_rename() -> testresult::TestResult {
    let context = schema(&[])?;
    let result = apply_ownership_sql(
        context,
        "CREATE TABLE child(id integer REFERENCES missing(id))",
    )?;
    assert!(!result.context().problems().is_empty());
    let context = apply_ownership_sql(
        result.into_context(),
        "CREATE TABLE missing(id integer PRIMARY KEY)",
    )?
    .into_context();
    let result = apply_ownership_sql(context, "ALTER TABLE missing RENAME id TO changed")?;
    let column = result
        .context()
        .get_table_impl(&name("child"))
        .unwrap()
        .get_column_at(0)
        .unwrap();
    let fk = column
        .constraints()
        .iter()
        .find_map(ColumnConstraint::as_foreign_key)
        .unwrap();
    assert!(fk.primary_key_table().is_none());
    assert_eq!(fk.primary_key_column().as_str(), "id");
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::view("CREATE VIEW v2 AS SELECT * FROM v1")]
#[case::subquery("CREATE VIEW v2 AS SELECT * FROM (SELECT a FROM t)")]
#[case::cte("CREATE VIEW v2 AS WITH q AS (SELECT a FROM t) SELECT * FROM q")]
fn sqlite_expanded_outputs_follow_their_source(#[case] view: &str) -> testresult::TestResult {
    use crate::{
        names::FullName,
        test_helpers::column_rename::{sqlite, sqlite_schema},
    };
    let statements = [
        "CREATE TABLE t(a)",
        "CREATE VIEW v1 AS SELECT * FROM t",
        view,
    ];
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    for sql in statements {
        oracle.execute_batch(sql)?;
    }
    oracle.execute_batch("ALTER TABLE t RENAME a TO c")?;
    oracle.prepare("SELECT c FROM v2")?;
    let result = sqlite(sqlite_schema(&statements)?, "ALTER TABLE t RENAME a TO c")?;
    assert!(
        result.ast().alter_table_statement().unwrap().can_apply(),
        "{:?}",
        result.context().problems()
    );
    assert_eq!(
        result
            .context()
            .get_view_impl(&FullName::with_schema("main", "v2"))
            .unwrap()
            .result_columns()
            .names(),
        vec![ColumnName::from("c")]
    );
    let result = sqlite(result.into_context(), "SELECT c FROM v2")?;
    assert!(result.context().problems().is_empty());
    Ok(())
}

#[test]
fn spelling_provenance_does_not_change_expression_equivalence() -> testresult::TestResult {
    let context = schema(&["CREATE TABLE t(a integer)", "ALTER TABLE t RENAME a TO c"])?;
    let result = apply_ownership_sql(context, "SELECT c AS k, t.c AS k FROM t ORDER BY k")?;
    assert!(result.context().problems().is_empty());
    Ok(())
}

#[test]
fn renamed_subscript_assignment_retains_the_updated_root_definition() -> testresult::TestResult {
    use crate::{
        context::RuleInfoRead,
        sem::{RuleIdentity, SemAst},
    };
    let context = schema(&[
        "CREATE TABLE t(a integer[])",
        "CREATE TABLE sink(a integer[])",
        "CREATE RULE forward AS ON UPDATE TO t DO ALSO UPDATE sink SET a[1] = NEW.a[1]",
        "ALTER TABLE sink RENAME a TO c",
    ])?;
    let rule = context
        .rule(&RuleIdentity::new(&name("t"), "forward"))
        .unwrap();
    let SemAst::UpdateStatement(update) = &rule.actions()[0] else {
        panic!("UPDATE action");
    };
    assert_eq!(
        update.assignments()[0]
            .column()
            .name()
            .as_ref()
            .unwrap()
            .as_str(),
        "c"
    );
    Ok(())
}

#[test]
fn expression_index_inference_uses_consistent_copied_column_constraints() -> testresult::TestResult
{
    let context = schema(&[
        "CREATE TABLE t(a integer[] NOT NULL)",
        "CREATE UNIQUE INDEX idx ON t((a[1]))",
        "ALTER TABLE t RENAME a TO c",
    ])?;
    let result = apply_ownership_sql(
        context,
        "INSERT INTO t(c) VALUES (ARRAY[1]) ON CONFLICT ((c[1])) DO NOTHING",
    )?;
    assert!(
        result.context().problems().is_empty(),
        "{:?}",
        result.context().problems()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case("CREATE VIEW v AS SELECT a IS NULL FROM t")]
#[case("CREATE VIEW v AS SELECT * FROM (SELECT a IS NULL FROM t)")]
#[should_panic(expected = "SQLite implicit expression-derived output names (#867)")]
fn sqlite_unfinished_implicit_expression_names_stop_explicitly(#[case] view: &str) {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let context = sqlite_schema(&["CREATE TABLE t(a)", view]).unwrap();
    let _ = sqlite(context, "ALTER TABLE t RENAME a TO c");
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::introduce("CREATE TABLE u(c)", "CREATE VIEW v AS SELECT * FROM t NATURAL JOIN u")]
#[case::remove("CREATE TABLE u(a)", "CREATE VIEW v AS SELECT * FROM t NATURAL JOIN u")]
#[case::joined_alias(
    "CREATE TABLE u(c)",
    "CREATE VIEW v AS SELECT j.* FROM (t NATURAL JOIN u) AS j"
)]
#[should_panic(expected = "SQLite NATURAL JOIN wildcard reconstruction (#868)")]
fn sqlite_unfinished_natural_star_shapes_stop_explicitly(#[case] other: &str, #[case] view: &str) {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let context = sqlite_schema(&["CREATE TABLE t(a)", other, view]).unwrap();
    let _ = sqlite(context, "ALTER TABLE t RENAME a TO c");
}

#[rstest::rstest]
#[case::expression("CREATE VIEW v AS SELECT a IS NULL FROM t")]
#[case::natural_star("CREATE VIEW v AS SELECT * FROM t NATURAL JOIN u")]
fn postgres_does_not_enter_sqlite_unfinished_paths(#[case] view: &str) -> testresult::TestResult {
    let context = schema(&[
        "CREATE TABLE t(a integer)",
        "CREATE TABLE u(c integer)",
        view,
    ])?;
    let outputs = context
        .get_view_impl(&name("v"))
        .unwrap()
        .result_columns()
        .names();
    let result = apply_ownership_sql(context, "ALTER TABLE t RENAME a TO c")?;
    assert!(result.ast().alter_table_statement().unwrap().can_apply());
    assert!(result.context().problems().is_empty());
    assert_eq!(
        result
            .context()
            .get_view_impl(&name("v"))
            .unwrap()
            .result_columns()
            .names(),
        outputs
    );
    assert!(
        result
            .context()
            .get_table_impl(&name("t"))
            .unwrap()
            .has_column(&ColumnName::from("c"))
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::explicit_output("CREATE VIEW v AS SELECT a IS NULL AS fixed FROM t")]
#[case::derived_alias("CREATE VIEW v AS WITH q(a) AS (SELECT a FROM t) SELECT a IS NULL FROM q")]
#[case::qualified_star("CREATE VIEW v AS SELECT t.* FROM t NATURAL JOIN u")]
#[case::stable_natural("CREATE VIEW v AS SELECT * FROM t AS x NATURAL JOIN t AS y")]
#[case::unrelated_expression("CREATE VIEW v AS SELECT c IS NULL FROM u")]
fn sqlite_correct_outputs_remain_supported(#[case] view: &str) -> testresult::TestResult {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::column_rename::{sqlite, sqlite_schema};
    let context = sqlite_schema(&["CREATE TABLE t(a)", "CREATE TABLE u(c)", view])?;
    let result = sqlite(context, "ALTER TABLE t RENAME a TO c")?;
    assert!(result.ast().alter_table_statement().unwrap().can_apply());
    assert!(result.context().problems().is_empty());
    Ok(())
}
