use crate::ast::{Dialect, DropBehavior, DropTrigger};
use crate::syn;
use crate::test_helpers::drop_syntax::postgres_drop_syntax;
use crate::test_helpers::drop_trigger_syntax::drop_trigger;

#[rstest::rstest]
#[case::bare("DROP TRIGGER TriggerName ON Items", &["items"], "triggername")]
#[case::schema("DROP TRIGGER t ON SchemaName.Items", &["schemaname", "items"], "t")]
#[case::catalog(
    r#"DROP TRIGGER "Trigger.Name" ON "Catalog"."Schema"."Items""#,
    &["Catalog", "Schema", "Items"],
    "Trigger.Name"
)]
#[case::unresolved_qualification("DROP TRIGGER t ON a.b.c.d", &["a", "b", "c", "d"], "t")]
#[case::escaped_quotes(r#"DROP TRIGGER "T""Name" ON "R""Name""#, &["R\"Name"], "T\"Name")]
fn postgres_preserves_target_names(
    #[case] sql: &str,
    #[case] relation: &[&str],
    #[case] name: &str,
) {
    let trigger = drop_trigger(sql, Dialect::postgres());
    assert_eq!(trigger.name.0, [name]);
    assert_eq!(trigger.relation.expect("explicit ON relation").0, relation);
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
    let trigger = drop_trigger(
        &format!("DROP TRIGGER {clause}t ON absent{suffix}"),
        Dialect::postgres(),
    );
    assert_eq!(trigger.if_exists, if_exists);
    assert_eq!(trigger.behavior, Some(behavior));
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::bare("TriggerName", &["TriggerName"])]
#[case::qualified("main.TriggerName", &["main", "TriggerName"])]
#[case::quoted(r#""Database"."Trigger.Name""#, &["Database", "Trigger.Name"])]
#[case::escaped_quotes(r#""T""Name""#, &["T\"Name"])]
fn sqlite_preserves_names_without_inventing_relation_or_behavior(
    #[case] target: &str,
    #[case] names: &[&str],
    #[values(false, true)] if_exists: bool,
) {
    let clause = if if_exists { "IF EXISTS " } else { "" };
    let trigger = drop_trigger(&format!("DROP TRIGGER {clause}{target}"), Dialect::sqlite());
    assert_eq!(trigger.name.0, names);
    assert_eq!(trigger.if_exists, if_exists);
    assert!(trigger.relation.is_none());
    assert!(trigger.behavior.is_none());
}

#[cfg(feature = "sqlite")]
#[test]
fn equivalent_concepts_use_common_shapes() {
    let postgres = drop_trigger(
        r#"DROP TRIGGER IF EXISTS "T" ON absent"#,
        Dialect::postgres(),
    );
    let sqlite = drop_trigger(r#"DROP TRIGGER IF EXISTS "T""#, Dialect::sqlite());
    assert_eq!(postgres.name, sqlite.name);
    assert_eq!(postgres.if_exists, sqlite.if_exists);
    assert!(postgres.relation.is_some());
    assert!(sqlite.relation.is_none());
    assert_eq!(postgres.behavior, Some(DropBehavior::restrict()));
    assert!(sqlite.behavior.is_none());
}

#[test]
#[should_panic(expected = "DROP TRIGGER object type")]
fn rejects_other_drop_kinds() {
    let syntax = syn::DropStmt::from(postgres_drop_syntax("DROP RULE t ON r"));
    let _ = DropTrigger::try_from(&syntax);
}

#[test]
#[should_panic(expected = "DROP TRIGGER cannot be concurrent")]
fn rejects_contradictory_concurrent_flag() {
    let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
    syntax.concurrent = true;
    let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
}

#[rstest::rstest]
#[case::empty(false)]
#[case::multiple(true)]
#[should_panic(expected = "one DROP TRIGGER target required")]
fn rejects_invalid_target_cardinality(#[case] multiple: bool) {
    let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
    if multiple {
        syntax.objects.extend(syntax.objects.clone());
    } else {
        syntax.objects.clear();
    }
    let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP TRIGGER relation and name")]
fn rejects_missing_relation_in_target() {
    let mut syntax = postgres_drop_syntax("DROP TABLE t");
    syntax.remove_type = ::pg_query::protobuf::ObjectType::ObjectTrigger as i32;
    let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP TRIGGER relation and name")]
fn rejects_non_list_target() {
    let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
    syntax.objects = vec![::pg_query::protobuf::Node::default()];
    let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
}

#[test]
#[should_panic(expected = "DROP behavior")]
fn rejects_unspecified_behavior() {
    let mut syntax = postgres_drop_syntax("DROP TRIGGER t ON r");
    syntax.behavior = 0;
    let _ = DropTrigger::try_from(&syn::DropStmt::from(syntax));
}

#[cfg(feature = "sqlite")]
#[test]
#[should_panic(expected = "DROP TRIGGER cannot have an alias")]
fn rejects_contradictory_sqlite_alias() {
    let mut name = ::turso_parser::ast::QualifiedName::single(::turso_parser::ast::Name::exact(
        "t".to_owned(),
    ));
    name.alias = Some(::turso_parser::ast::Name::exact("alias".to_owned()));
    let syntax =
        syn::sqlite::DropTriggerStatement::new(false, syn::sqlite::QualifiedName::from(name));
    let _ = DropTrigger::try_from(&syntax);
}
