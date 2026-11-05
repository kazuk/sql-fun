#[cfg(feature = "sqlite")]
use crate::ast::QueryBody;
use crate::{
    ast::{Dialect, Expr, StatementNode, TriggerBody, TriggerEvent},
    test_helpers::trigger::trigger,
};

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
    let QueryBody::Select(select) = &query.body else {
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
