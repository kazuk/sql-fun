use crate::{
    PostgresDialect,
    ast::{CreateRule, Expr, QueryBody, RuleEvent, SetOperation, StatementNode, TryFromWithTokens},
    syn,
    test_helpers::rule::{postgres_rule_syntax, rule},
};

impl Expr {
    fn assert_test_rule_condition(&self) {
        let Self::Operator(operator) = self else {
            panic!("comparison")
        };
        assert_eq!(operator.name.0, [">"]);
        let Some(Self::Cast(cast)) = &operator.left else {
            panic!("cast left operand")
        };
        assert_eq!(cast.target.name.0, ["pg_catalog", "numeric"]);
        assert_eq!(cast.target.modifiers.len(), 2);
        cast.argument.assert_test_column(&["new", "Value"]);
        operator
            .right
            .as_ref()
            .expect("right operand")
            .assert_test_column(&["old", "Value"]);
    }

    fn assert_test_column(&self, expected: &[&str]) {
        let Self::Column(name) = self else {
            panic!("column reference")
        };
        assert_eq!(name.0, expected);
    }
}

impl StatementNode {
    fn assert_test_select_new_id(&self) {
        let Self::Query(query) = self else {
            panic!("query action")
        };
        query.body.assert_test_select_new_id();
    }
}

impl QueryBody {
    fn assert_test_select_new_id(&self) {
        let Self::Select(select) = self else {
            panic!("SELECT body")
        };
        assert_eq!(select.targets.len(), 1);
        select.targets[0]
            .expression
            .assert_test_column(&["new", "id"]);
    }
}

#[test]
fn preserves_ordered_dml_and_query_actions() {
    let rule = rule(
        r#"CREATE RULE r AS ON UPDATE TO absent DO ALSO (
          ; INSERT INTO "S"."Items" (id) OVERRIDING SYSTEM VALUE SELECT NEW.id
            ON CONFLICT (id) WHERE id > 0 DO UPDATE SET id = excluded.id
            WHERE excluded.id > 1 RETURNING id AS "Inserted";
          UPDATE ONLY items AS i SET id = NEW.id FROM source AS s
            WHERE i.id = OLD.id RETURNING i.id AS "Updated";
          DELETE FROM items AS d USING source AS s
            WHERE d.id = OLD.id RETURNING d.id AS "Deleted";
          SELECT NEW.id;;
        )"#,
    );
    assert_eq!(rule.actions.len(), 4);
    rule.actions[0].assert_test_insert_action();
    rule.actions[1].assert_test_update_action();
    rule.actions[2].assert_test_delete_action();
    rule.actions[3].assert_test_select_new_id();
}

impl StatementNode {
    fn assert_test_insert_action(&self) {
        let Self::Insert(insert) = self else {
            panic!("INSERT first")
        };
        assert_eq!(insert.relation.name.0, ["S", "Items"]);
        assert_eq!(insert.columns[0].name, "id");
        assert!(insert.overriding.is_some());
        insert
            .source
            .as_ref()
            .expect("INSERT SELECT")
            .body
            .assert_test_select_new_id();
        let conflict = insert.conflict.as_ref().expect("ON CONFLICT");
        assert_eq!(conflict.inference[0].name.as_deref(), Some("id"));
        assert!(conflict.inference_predicate.is_some());
        assert_eq!(conflict.assignments[0].name, "id");
        conflict.assignments[0]
            .value
            .as_ref()
            .expect("SET value")
            .assert_test_column(&["excluded", "id"]);
        assert!(conflict.predicate.is_some());
        assert_eq!(insert.returning[0].alias.as_deref(), Some("Inserted"));
        insert.returning[0].expression.assert_test_column(&["id"]);
    }

    fn assert_test_update_action(&self) {
        let Self::Update(update) = self else {
            panic!("UPDATE second")
        };
        assert_eq!(update.relation.name.0, ["items"]);
        assert!(!update.relation.inherit);
        assert_eq!(
            update.relation.alias.as_ref().expect("target alias").name,
            "i"
        );
        assert_eq!(update.assignments[0].name, "id");
        update.assignments[0]
            .value
            .as_ref()
            .expect("SET value")
            .assert_test_column(&["new", "id"]);
        assert_eq!(update.from.len(), 1);
        assert!(update.predicate.is_some());
        assert_eq!(update.returning[0].alias.as_deref(), Some("Updated"));
        update.returning[0]
            .expression
            .assert_test_column(&["i", "id"]);
    }

    fn assert_test_delete_action(&self) {
        let Self::Delete(delete) = self else {
            panic!("DELETE third")
        };
        assert_eq!(delete.relation.name.0, ["items"]);
        assert_eq!(delete.using.len(), 1);
        assert!(delete.predicate.is_some());
        assert_eq!(delete.returning[0].alias.as_deref(), Some("Deleted"));
        delete.returning[0]
            .expression
            .assert_test_column(&["d", "id"]);
    }
}

#[test]
fn preserves_nested_query_and_condition_syntax() {
    let rule = rule(
        r#"CREATE RULE r AS ON SELECT TO absent
        WHERE EXISTS (SELECT 1 FROM source WHERE id = OLD.id)
        DO INSTEAD WITH c(x) AS MATERIALIZED (VALUES (1))
        SELECT DISTINCT ON (x) x::int AS value, row_number() OVER w
        FROM c WHERE x > 0 GROUP BY x HAVING count(*) > 0
        WINDOW w AS (ORDER BY x ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)
        ORDER BY x DESC NULLS FIRST LIMIT 3 OFFSET 1"#,
    );
    assert_eq!(rule.event, RuleEvent::select());
    rule.predicate
        .as_ref()
        .expect("EXISTS")
        .assert_test_subquery();
    rule.actions[0].assert_test_query_clauses();
}

impl Expr {
    fn assert_test_subquery(&self) {
        let Self::Subquery(subquery) = self else {
            panic!("subquery")
        };
        subquery.query.body.assert_test_filtered_query();
    }
}

impl QueryBody {
    fn assert_test_filtered_query(&self) {
        let Self::Select(select) = self else {
            panic!("SELECT")
        };
        assert_eq!(select.from.len(), 1);
        assert!(select.predicate.is_some());
    }
}

impl StatementNode {
    fn assert_test_query_clauses(&self) {
        let Self::Query(query) = self else {
            panic!("query action")
        };
        assert_eq!(query.with.entries[0].name, "c");
        assert_eq!(query.with.entries[0].columns[0].name, "x");
        assert_eq!(query.with.entries[0].materialized, Some(true));
        assert_eq!(query.order.len(), 1);
        assert_eq!(query.order[0].descending, Some(true));
        assert_eq!(query.order[0].nulls_first, Some(true));
        assert!(query.limit.is_some());
        assert!(query.offset.is_some());
        query.body.assert_test_query_clauses();
    }
}

impl QueryBody {
    fn assert_test_query_clauses(&self) {
        let Self::Select(select) = self else {
            panic!("SELECT")
        };
        assert_eq!(select.distinct.as_ref().expect("DISTINCT ON").len(), 1);
        assert_eq!(select.targets.len(), 2);
        assert_eq!(select.targets[0].alias.as_deref(), Some("value"));
        assert_eq!(select.from.len(), 1);
        assert!(select.predicate.is_some());
        assert_eq!(select.group.len(), 1);
        assert!(select.having.is_some());
        assert_eq!(select.windows.len(), 1);
        assert!(select.windows[0].frame.is_some());
    }
}

#[test]
fn retains_nested_type_diagnostic_locations() {
    let sql = r#"CREATE RULE r AS ON INSERT TO t WHERE NEW.id::"Types"."Number"(12,3) > 0 DO SELECT NEW.id::"Types"."Text"(7)"#;
    let frontend = postgres_rule_syntax(sql);
    let tokens = PostgresDialect::scan(sql).expect("tokens");
    let rule = CreateRule::try_from_with_tokens(&syn::RuleStmt::from(frontend), &tokens)
        .expect("conversion");
    rule.predicate
        .as_ref()
        .expect("predicate")
        .assert_test_cast_location(sql, r#""Types"."Number""#);
    rule.actions[0].assert_test_cast_location(sql);
}

impl Expr {
    fn assert_test_cast_location(&self, sql: &str, expected: &str) {
        let cast = match self {
            Self::Cast(cast) => cast,
            Self::Operator(operator) => {
                operator
                    .left
                    .as_ref()
                    .expect("left")
                    .assert_test_cast_location(sql, expected);
                return;
            }
            _ => panic!("cast expression"),
        };
        let location = cast.target.location.as_ref().expect("type location");
        assert_eq!(location.get_from(sql), Some(expected));
    }
}

impl StatementNode {
    fn assert_test_cast_location(&self, sql: &str) {
        let Self::Query(query) = self else {
            panic!("SELECT action")
        };
        query.body.assert_test_cast_location(sql);
    }
}

impl QueryBody {
    fn assert_test_cast_location(&self, sql: &str) {
        let Self::Select(select) = self else {
            panic!("SELECT")
        };
        select.targets[0]
            .expression
            .assert_test_cast_location(sql, r#""Types"."Text""#);
    }
}

#[rstest::rstest]
#[case::union("UNION", false)]
#[case::union_all("UNION ALL", true)]
#[case::intersect("INTERSECT", false)]
#[case::except("EXCEPT", false)]
fn preserves_set_actions_and_values(#[case] operation: &str, #[case] all: bool) {
    let rule = rule(&format!(
        "CREATE RULE r AS ON SELECT TO absent DO INSTEAD VALUES (1) {operation} VALUES (2)"
    ));
    rule.actions[0].assert_test_set_action(operation, all);
}

impl StatementNode {
    fn assert_test_set_action(&self, operation: &str, all: bool) {
        let Self::Query(query) = self else {
            panic!("query")
        };
        query.body.assert_test_set_action(operation, all);
    }
}

impl QueryBody {
    fn assert_test_set_action(&self, operation: &str, all: bool) {
        let Self::Set(set) = self else {
            panic!("set operation")
        };
        set.operation.assert_test_operation(operation);
        assert_eq!(set.all, all);
        set.left.body.assert_test_values();
        set.right.body.assert_test_values();
    }

    fn assert_test_values(&self) {
        let Self::Values(rows) = self else {
            panic!("VALUES")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 1);
    }
}

impl SetOperation {
    fn assert_test_operation(&self, operation: &str) {
        let name = match self {
            Self::Union => "UNION",
            Self::Intersect => "INTERSECT",
            Self::Except => "EXCEPT",
        };
        assert_eq!(operation.split_whitespace().next(), Some(name));
    }
}

#[test]
fn preserves_identity_replacement_and_complete_condition() {
    let rule = rule(
        r#"CREATE OR REPLACE RULE "Rule" AS ON UPDATE TO "Catalog"."Schema"."Target"
        WHERE NEW."Value"::numeric(12, 3) > OLD."Value" DO INSTEAD NOTHING"#,
    );
    assert_eq!(rule.name.0, ["Rule"]);
    assert_eq!(rule.relation.name.0, ["Catalog", "Schema", "Target"]);
    assert!(rule.relation.alias.is_none());
    assert!(rule.relation.inherit);
    assert_eq!(rule.relation.persistence, "p");
    assert!(rule.replace);
    assert!(rule.instead);
    assert_eq!(rule.event, RuleEvent::update());
    let predicate = rule.predicate.expect("WHERE condition");
    predicate.assert_test_rule_condition();
    assert!(rule.actions.is_empty());
}

#[rstest::rstest]
#[case::omitted("", false)]
#[case::also("ALSO", false)]
#[case::instead("INSTEAD", true)]
fn preserves_action_mode(#[case] mode: &str, #[case] instead: bool) {
    let rule = rule(&format!(
        "CREATE RULE r AS ON INSERT TO s.t DO {mode} SELECT NEW.id"
    ));
    assert_eq!(rule.relation.name.0, ["s", "t"]);
    assert_eq!(rule.instead, instead);
    assert_eq!(rule.actions.len(), 1);
    rule.actions[0].assert_test_select_new_id();
}
