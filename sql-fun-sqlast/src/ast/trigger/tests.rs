use crate::ast::{
    CreateTrigger, Dialect, Expr, StatementKind, StatementNode, TriggerBody, TriggerEvent,
    TriggerGranularity, TriggerTiming,
};
use crate::parse_statements;

impl StatementNode {
    fn into_test_trigger(self) -> CreateTrigger {
        match self {
            Self::CreateTrigger(trigger) => trigger,
            _ => panic!("CREATE TRIGGER node"),
        }
    }
}

fn trigger(sql: &str, dialect: Dialect) -> CreateTrigger {
    let statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("context-free conversion");
    assert_eq!(statement.kind(), StatementKind::create_trigger());
    assert!(statement.with_clause().is_none());
    statement.node.into_test_trigger()
}

#[test]
fn postgres_preserves_identity_flags_and_function_arguments() {
    let trigger = trigger(
        r#"CREATE OR REPLACE TRIGGER "T" BEFORE INSERT OR UPDATE OF "Value", other
        ON "Catalog"."Schema"."Items" FOR EACH ROW
        WHEN (NEW."Value" > 0) EXECUTE FUNCTION "Fns"."Run"('a''b', 42, identifier)"#,
        Dialect::postgres(),
    );
    assert_eq!(trigger.name.0, ["T"]);
    assert_eq!(trigger.relation.name.0, ["Catalog", "Schema", "Items"]);
    assert!(trigger.replace);
    assert!(!trigger.if_not_exists);
    assert!(!trigger.temporary);
    assert_eq!(trigger.timing, Some(TriggerTiming::Before));
    assert_eq!(trigger.granularity, Some(TriggerGranularity::Row));
    assert_eq!(
        trigger.events,
        [
            TriggerEvent::Insert,
            TriggerEvent::Update(vec!["Value".into(), "other".into()])
        ]
    );
    assert!(matches!(trigger.predicate, Some(Expr::Operator(_))));
    assert!(trigger.constraint.is_none());
    assert!(trigger.transitions.is_empty());
    let TriggerBody::Function(function) = trigger.body else {
        panic!("function body")
    };
    assert_eq!(function.name.0, ["Fns", "Run"]);
    assert_eq!(function.arguments, ["a'b", "42", "identifier"]);
}

#[rstest::rstest]
#[case::after_omitted_level(
    "AFTER INSERT",
    "",
    TriggerTiming::After,
    TriggerGranularity::Statement
)]
#[case::before_statement(
    "BEFORE DELETE",
    "FOR EACH STATEMENT",
    TriggerTiming::Before,
    TriggerGranularity::Statement
)]
#[case::instead_row(
    "INSTEAD OF UPDATE",
    "FOR EACH ROW",
    TriggerTiming::InsteadOf,
    TriggerGranularity::Row
)]
fn postgres_timing_and_granularity(
    #[case] event: &str,
    #[case] level: &str,
    #[case] timing: TriggerTiming,
    #[case] granularity: TriggerGranularity,
) {
    let trigger = trigger(
        &format!("CREATE TRIGGER t {event} ON absent {level} EXECUTE PROCEDURE f()"),
        Dialect::postgres(),
    );
    assert_eq!(trigger.timing, Some(timing));
    assert_eq!(trigger.granularity, Some(granularity));
    let TriggerBody::Function(function) = trigger.body else {
        panic!("function body")
    };
    assert!(function.arguments.is_empty());
}

#[rstest::rstest]
#[case::table("TABLE", true)]
#[case::row("ROW", false)]
fn postgres_preserves_all_events_and_transition_relations(
    #[case] relation_kind: &str,
    #[case] table: bool,
) {
    let trigger = trigger(
        &format!(
            r#"CREATE TRIGGER t AFTER INSERT OR DELETE OR UPDATE OR TRUNCATE ON missing
        REFERENCING OLD {relation_kind} AS "Before" NEW {relation_kind} AS "After"
        FOR EACH STATEMENT EXECUTE FUNCTION f()"#
        ),
        Dialect::postgres(),
    );
    assert_eq!(
        trigger.events,
        [
            TriggerEvent::Insert,
            TriggerEvent::Delete,
            TriggerEvent::Update(vec![]),
            TriggerEvent::Truncate
        ]
    );
    assert_eq!(trigger.transitions.len(), 2);
    assert_eq!(trigger.transitions[0].name, "Before");
    assert!(!trigger.transitions[0].new);
    assert_eq!(trigger.transitions[0].table, table);
    assert_eq!(trigger.transitions[1].name, "After");
    assert!(trigger.transitions[1].new);
    assert_eq!(trigger.transitions[1].table, table);
}

#[rstest::rstest]
#[case::default("", "", false, false, false)]
#[case::not_deferrable("", "NOT DEFERRABLE INITIALLY IMMEDIATE", false, false, false)]
#[case::deferred(
    "FROM public.source",
    "DEFERRABLE INITIALLY DEFERRED",
    true,
    true,
    true
)]
#[case::immediate(
    "FROM public.source",
    "DEFERRABLE INITIALLY IMMEDIATE",
    true,
    true,
    false
)]
fn postgres_constraint_options(
    #[case] from: &str,
    #[case] options: &str,
    #[case] has_from: bool,
    #[case] deferrable: bool,
    #[case] deferred: bool,
) {
    let trigger = trigger(
        &format!(
            "CREATE CONSTRAINT TRIGGER t AFTER UPDATE ON public.missing {from} {options} FOR EACH ROW WHEN (NEW.id IS NOT NULL) EXECUTE FUNCTION f()"
        ),
        Dialect::postgres(),
    );
    let constraint = trigger.constraint.expect("constraint syntax");
    assert_eq!(constraint.relation.is_some(), has_from);
    if let Some(relation) = constraint.relation {
        assert_eq!(relation.name.0, ["public", "source"]);
    }
    assert_eq!(constraint.deferrable, deferrable);
    assert_eq!(constraint.initially_deferred, deferred);
    assert!(trigger.predicate.is_some());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::omitted("", "", None, None)]
#[case::before(
    "BEFORE",
    "FOR EACH ROW",
    Some(TriggerTiming::Before),
    Some(TriggerGranularity::Row)
)]
#[case::after("AFTER", "", Some(TriggerTiming::After), None)]
#[case::instead(
    "INSTEAD OF",
    "FOR EACH ROW",
    Some(TriggerTiming::InsteadOf),
    Some(TriggerGranularity::Row)
)]
fn sqlite_preserves_creation_and_optional_clauses(
    #[case] time: &str,
    #[case] level: &str,
    #[case] timing: Option<TriggerTiming>,
    #[case] granularity: Option<TriggerGranularity>,
) {
    let trigger = trigger(
        &format!(
            r#"CREATE TEMP TRIGGER IF NOT EXISTS "temp"."T" {time} UPDATE OF "Value", other ON "main"."Missing" {level} WHEN NEW."Value" > 0 BEGIN SELECT 1; END"#
        ),
        Dialect::sqlite(),
    );
    assert_eq!(trigger.name.0, ["temp", "T"]);
    assert_eq!(trigger.relation.name.0, ["main", "Missing"]);
    assert!(trigger.temporary);
    assert!(trigger.if_not_exists);
    assert!(!trigger.replace);
    assert_eq!(trigger.timing, timing);
    assert_eq!(trigger.granularity, granularity);
    assert_eq!(
        trigger.events,
        [TriggerEvent::Update(vec!["Value".into(), "other".into()])]
    );
    assert!(trigger.predicate.is_some());
    assert!(trigger.constraint.is_none());
    assert!(trigger.transitions.is_empty());
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::insert("INSERT", TriggerEvent::Insert)]
#[case::update("UPDATE", TriggerEvent::Update(vec![]))]
#[case::delete("DELETE", TriggerEvent::Delete)]
fn equivalent_events_use_common_shapes(#[case] event: &str, #[case] expected: TriggerEvent) {
    let postgres = trigger(
        &format!("CREATE TRIGGER t BEFORE {event} ON missing FOR EACH ROW EXECUTE FUNCTION f()"),
        Dialect::postgres(),
    );
    let sqlite = trigger(
        &format!("CREATE TRIGGER t BEFORE {event} ON missing FOR EACH ROW BEGIN SELECT 1; END"),
        Dialect::sqlite(),
    );
    assert_eq!(postgres.name.0, sqlite.name.0);
    assert_eq!(postgres.relation.name.0, sqlite.relation.name.0);
    assert_eq!(postgres.timing, sqlite.timing);
    assert_eq!(postgres.granularity, sqlite.granularity);
    assert_eq!(postgres.events, sqlite.events);
    assert_eq!(sqlite.events, [expected]);
    assert!(!sqlite.temporary);
    assert!(!sqlite.if_not_exists);
    assert!(sqlite.predicate.is_none());
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_preserves_ordered_common_command_payloads() {
    let trigger = trigger(
        r#"
        CREATE TRIGGER t AFTER INSERT ON missing BEGIN
            UPDATE OR FAIL items SET (a, b) = (SELECT NEW.a, NEW.b), c = NEW.c
                FROM source WHERE items.id = source.id;
            INSERT OR IGNORE INTO audit(a, b) SELECT NEW.a, NEW.b
                ON CONFLICT(a) WHERE a > 0 DO UPDATE SET b = excluded.b WHERE b > 0
                ON CONFLICT DO NOTHING;
            DELETE FROM obsolete WHERE id IN (SELECT OLD.id);
            SELECT CASE WHEN NEW.a IS NULL THEN RAISE(ABORT, 'missing') ELSE NEW.a END;
        END
    "#,
        Dialect::sqlite(),
    );
    let TriggerBody::Commands(commands) = trigger.body else {
        panic!("command body")
    };
    assert_eq!(commands.len(), 4);
    let StatementNode::Update(update) = &commands[0] else {
        panic!("UPDATE")
    };
    assert_eq!(update.relation.name.0, ["items"]);
    assert!(update.or_conflict.is_some());
    assert_eq!(update.assignments.len(), 3);
    assert!(matches!(
        update.assignments[0].value,
        Some(Expr::Document(_))
    ));
    assert_eq!(update.from.len(), 1);
    assert!(update.predicate.is_some());
    let StatementNode::Insert(insert) = &commands[1] else {
        panic!("INSERT")
    };
    assert_eq!(insert.relation.name.0, ["audit"]);
    assert_eq!(insert.columns.len(), 2);
    assert!(insert.source.is_some());
    assert!(insert.or_conflict.is_some());
    let conflict = insert.conflict.as_ref().expect("upsert");
    assert_eq!(conflict.inference.len(), 1);
    assert!(conflict.inference_predicate.is_some());
    assert_eq!(conflict.assignments.len(), 1);
    assert!(conflict.predicate.is_some());
    assert!(conflict.next.is_some());
    assert!(insert.returning.is_empty());
    let StatementNode::Delete(delete) = &commands[2] else {
        panic!("DELETE")
    };
    assert_eq!(delete.relation.name.0, ["obsolete"]);
    assert!(matches!(delete.predicate, Some(Expr::Subquery(_))));
    let StatementNode::Query(query) = &commands[3] else {
        panic!("query")
    };
    let crate::ast::QueryBody::Select(select) = &query.body else {
        panic!("SELECT")
    };
    let Expr::Case(case) = &select.targets[0].expression else {
        panic!("CASE")
    };
    assert!(matches!(case.arms[0].result, Expr::Raise(_)));
}

#[cfg(all(feature = "sqlite", feature = "workaround_turso_9791"))]
#[test]
fn sqlite_retains_type_spelling_in_when_and_every_command() {
    let trigger = trigger(
        r#"CREATE TRIGGER t INSERT ON missing
        WHEN CAST(NEW.a AS "BLOB" INTEGER) > 0 BEGIN
        UPDATE items SET a = CAST(NEW.a AS [BLOB] TEXT);
        INSERT INTO items VALUES (CAST(NEW.a AS BLOB/*TEXT*/REAL));
        DELETE FROM items WHERE CAST(a AS NUMERIC(10, 2)) > 0;
        SELECT CAST(NEW.a AS DOUBLE "PRECISION");
        END"#,
        Dialect::sqlite(),
    );
    let Expr::Operator(predicate) = trigger.predicate.expect("WHEN") else {
        panic!("operator")
    };
    let Expr::Cast(cast) = predicate.left.as_ref().expect("cast operand") else {
        panic!("CAST")
    };
    assert_eq!(cast.target.name.0, ["BLOB INTEGER"]);
    assert_eq!(cast.target.sqlite_parts[0].quote, Some('"'));
    assert_eq!(cast.target.sqlite_parts[0].name, "BLOB");
    let TriggerBody::Commands(commands) = trigger.body else {
        panic!("command body")
    };
    assert_eq!(commands.len(), 4);
}
