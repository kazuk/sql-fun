use crate::ast::{Dialect, DropBehavior, DropRule, StatementKind, StatementNode};
use crate::syn::Opt;
use crate::{parse_statements, syn};

impl StatementNode {
    fn into_test_drop_rule(self) -> DropRule {
        match self {
            Self::DropRule(rule) => rule,
            _ => panic!("DROP RULE node"),
        }
    }
}

fn drop_rule(sql: &str, dialect: Dialect) -> DropRule {
    let statement = parse_statements(sql, dialect)
        .next()
        .expect("statement")
        .expect("context-free DROP RULE conversion");
    assert_eq!(statement.kind(), StatementKind::drop_rule());
    statement.node.into_test_drop_rule()
}

#[rstest::rstest]
#[case::bare("DROP RULE RuleName ON Items", &["items"], "rulename")]
#[case::schema("DROP RULE t ON SchemaName.Items", &["schemaname", "items"], "t")]
#[case::catalog(
    r#"DROP RULE "Rule.Name" ON "Catalog"."Schema"."Items""#,
    &["Catalog", "Schema", "Items"],
    "Rule.Name"
)]
#[case::unresolved_qualification("DROP RULE t ON a.b.c.d", &["a", "b", "c", "d"], "t")]
#[case::escaped_quotes(r#"DROP RULE "T""Name" ON "R""Name""#, &["R\"Name"], "T\"Name")]
fn postgres_preserves_target_names(
    #[case] sql: &str,
    #[case] relation: &[&str],
    #[case] name: &str,
) {
    let rule = drop_rule(sql, Dialect::postgres());
    assert_eq!(rule.name.0, [name]);
    assert_eq!(rule.relation.0, relation);
}

#[rstest::rstest]
#[case::implicit_restrict("", DropBehavior::restrict())]
#[case::explicit_restrict(" RESTRICT", DropBehavior::restrict())]
#[case::cascade(" CASCADE", DropBehavior::cascade())]
fn postgres_preserves_drop_options(
    #[case] suffix: &str,
    #[case] behavior: DropBehavior,
    #[values(false, true)] if_exists: bool,
) {
    let clause = if if_exists { "IF EXISTS " } else { "" };
    let rule = drop_rule(
        &format!("DROP RULE {clause}t ON absent{suffix}"),
        Dialect::postgres(),
    );
    assert_eq!(rule.if_exists, if_exists);
    assert_eq!(rule.behavior, behavior);
}

fn postgres_drop_syntax(sql: &str) -> ::pg_query::protobuf::DropStmt {
    let result =
        ::pg_query::parse(sql, ::pg_query::ParserOptions::default()).expect("DROP syntax fixture");
    let node = result
        .protobuf
        .stmts
        .into_iter()
        .next()
        .expect("statement")
        .stmt
        .expect("node")
        .node
        .expect("payload");
    match node {
        ::pg_query::protobuf::node::Node::DropStmt(value) => value,
        _ => panic!("DROP statement fixture"),
    }
}

#[rstest::rstest]
#[case::trigger("DROP TRIGGER t ON r")]
#[case::table("DROP TABLE r")]
#[should_panic(expected = "DROP RULE object type")]
fn rejects_other_drop_kinds(#[case] sql: &str) {
    let syntax = syn::DropStmt::from(postgres_drop_syntax(sql));
    let _ = DropRule::try_from(&syntax);
}

#[test]
#[should_panic(expected = "DROP RULE cannot be concurrent")]
fn rejects_contradictory_concurrent_flag() {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    syntax.concurrent = true;
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[rstest::rstest]
#[case::empty(false)]
#[case::multiple(true)]
#[should_panic(expected = "one DROP RULE target required")]
fn rejects_invalid_target_cardinality(#[case] multiple: bool) {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    if multiple {
        syntax.objects.extend(syntax.objects.clone());
    } else {
        syntax.objects.clear();
    }
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP RULE relation and name")]
fn rejects_missing_relation_in_target() {
    let mut syntax = postgres_drop_syntax("DROP TABLE t");
    syntax.remove_type = ::pg_query::protobuf::ObjectType::ObjectRule as i32;
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP RULE relation and name")]
fn rejects_non_list_target() {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    syntax.objects = vec![::pg_query::protobuf::Node::default()];
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[rstest::rstest]
#[case::undefined(0)]
#[case::invalid(999)]
#[should_panic(expected = "DROP behavior")]
fn rejects_unspecified_behavior(#[case] behavior: i32) {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    syntax.behavior = behavior;
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP RULE object type")]
fn rejects_unspecified_object_type() {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    syntax.remove_type = 0;
    let _ = DropRule::try_from(&syn::DropStmt::from(syntax));
}

#[test]
fn rejects_non_string_name_component() {
    let mut syntax = postgres_drop_syntax("DROP RULE t ON r");
    let Some(::pg_query::protobuf::node::Node::List(list)) = &mut syntax.objects[0].node else {
        panic!("DROP RULE name list");
    };
    list.items[0] = ::pg_query::protobuf::Node {
        node: Some(::pg_query::protobuf::node::Node::Integer(
            ::pg_query::protobuf::Integer { ival: 1 },
        )),
    };
    assert!(DropRule::try_from(&syn::DropStmt::from(syntax)).is_err());
}

#[rstest::rstest]
#[case::minimal("DROP RULE r ON absent")]
#[case::full(r#"DROP RULE IF EXISTS "Rule.Name" ON "Catalog"."Schema"."Relation" CASCADE"#)]
fn frontend_fields_agree_with_common_ast(#[case] sql: &str) {
    let frontend = postgres_drop_syntax(sql);
    let common = drop_rule(sql, Dialect::postgres());
    assert_eq!(frontend.objects.len(), 1);
    let Some(::pg_query::protobuf::node::Node::List(list)) = &frontend.objects[0].node else {
        panic!("frontend name list");
    };
    let mut parts = Vec::new();
    for item in &list.items {
        let Some(::pg_query::protobuf::node::Node::String(value)) = &item.node else {
            panic!("frontend name component");
        };
        parts.push(value.sval.clone());
    }
    assert_eq!(common.name.0, [parts.pop().expect("rule name")]);
    assert_eq!(common.relation.0, parts);
    assert_eq!(common.if_exists, frontend.missing_ok);
    assert_eq!(
        common.behavior,
        DropBehavior::try_from(syn::DropBehaviorOpt::from(frontend.behavior).as_inner())
            .expect("drop behavior"),
    );
    assert_eq!(
        frontend.remove_type,
        ::pg_query::protobuf::ObjectType::ObjectRule as i32
    );
    assert!(!frontend.concurrent);
}
