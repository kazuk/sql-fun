use super::test_helpers::catalog;
use crate::ast::Dialect;
use crate::sem::{CursorInfo, FullName, ParseContext, SemAst, TableName};
use crate::test_helpers::TestParseContext;

fn resolve(sql: &str, context: &mut TestParseContext) -> SemAst {
    crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(context)
        .unwrap()
}

#[rstest::rstest]
#[case::target(Some("items"), Some(true), false)]
#[case::unknown_target(None, Some(true), false)]
#[case::unknown_eligibility(Some("items"), None, false)]
#[case::unknown_facts(None, None, false)]
#[case::wrong_target(Some("other"), Some(true), true)]
#[case::read_only(Some("items"), Some(false), true)]
fn resolves_portal_in_current_context(
    #[case] target: Option<&str>,
    #[case] updatable: Option<bool>,
    #[case] invalid: bool,
) {
    let mut context = catalog(Dialect::postgres());
    let target = target.map(|name| TableName::from(FullName::with_schema("public", name)));
    let info = CursorInfo::new(target.as_ref(), updatable);
    context.bind_cursor("portal", &info);
    let statement = resolve(
        "DELETE FROM items AS i WHERE CURRENT OF portal RETURNING $1::integer",
        &mut context,
    );
    let delete = statement.as_delete().unwrap();
    assert!(delete.predicate().is_none());
    let cursor = delete.current_of().unwrap();
    assert_eq!(cursor.name(), "portal");
    assert_eq!(cursor.binding(), Some(&info));
    assert_eq!(
        context.reported_problem_count() > 0,
        invalid,
        "{:?}",
        context.reported_problems()
    );
    assert!(
        context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&crate::sem::PgBuiltInType::int4())
    );
}

#[rstest::rstest]
#[case::empty_scope(false)]
#[case::different_portal(true)]
fn retains_missing_reference(#[case] other_binding: bool) {
    let mut context = catalog(Dialect::postgres());
    if other_binding {
        context.bind_cursor("other", &CursorInfo::new(None, None));
    }
    let statement = resolve("DELETE FROM items WHERE CURRENT OF absent", &mut context);
    let cursor = statement.as_delete().unwrap().current_of().unwrap();
    assert_eq!(cursor.name(), "absent");
    assert!(cursor.binding().is_none());
    assert_eq!(context.reported_problem_count(), 1);
    assert!(context.reported_problems()[0].delete_problem().is_some());
}

#[test]
fn portal_scope_survives_statement_reset_and_can_be_replaced() {
    let mut context = catalog(Dialect::postgres());
    context.bind_cursor("portal", &CursorInfo::new(None, None));
    let first = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
    let second = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
    assert!(
        first
            .as_delete()
            .unwrap()
            .current_of()
            .unwrap()
            .binding()
            .is_some()
    );
    assert!(
        second
            .as_delete()
            .unwrap()
            .current_of()
            .unwrap()
            .binding()
            .is_some()
    );
    let replacement = CursorInfo::new(None, Some(false));
    context.bind_cursor("portal", &replacement);
    let third = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
    assert_eq!(
        third.as_delete().unwrap().current_of().unwrap().binding(),
        Some(&replacement)
    );
    // Semantic results are snapshots, independent of subsequent scope changes.
    assert_eq!(
        first
            .as_delete()
            .unwrap()
            .current_of()
            .unwrap()
            .binding()
            .unwrap()
            .updatable(),
        None
    );
    context.remove_cursor("portal");
    let last = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
    assert!(
        last.as_delete()
            .unwrap()
            .current_of()
            .unwrap()
            .binding()
            .is_none()
    );
}

#[rstest::rstest]
#[case::quoted(r#"DELETE FROM items WHERE CURRENT OF "Mixed""#, "Mixed", false)]
#[case::unquoted("DELETE FROM items WHERE CURRENT OF Mixed", "mixed", true)]
fn portal_names_follow_identifier_rules(
    #[case] sql: &str,
    #[case] name: &str,
    #[case] missing: bool,
) {
    let mut context = catalog(Dialect::postgres());
    context.bind_cursor("Mixed", &CursorInfo::new(None, None));
    let statement = resolve(sql, &mut context);
    let cursor = statement.as_delete().unwrap().current_of().unwrap();
    assert_eq!(cursor.name(), name);
    assert_eq!(cursor.binding().is_none(), missing);
}
