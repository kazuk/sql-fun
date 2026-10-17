use crate::ast::Dialect;
use crate::sem::{FullName, ViewDeleteSupport, ViewLike};

#[rstest::rstest]
#[case::writable("DELETE FROM writable_items WHERE $1 RETURNING *", true, true, true)]
#[case::cte_shadow(
    "WITH writable_items AS (SELECT 2 other) DELETE FROM writable_items RETURNING *",
    true,
    true,
    true
)]
#[case::read_only("DELETE FROM writable_items", false, false, false)]
#[case::returning("DELETE FROM writable_items RETURNING id", true, false, false)]
fn respects_delete_capabilities(
    #[case] sql: &str,
    #[case] writable: bool,
    #[case] returning: bool,
    #[case] valid: bool,
) {
    let mut context = super::test_helpers::catalog(Dialect::postgres());
    let view_query = crate::parse_statements("SELECT id,label FROM items", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context)
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "writable_items");
    let view = crate::sem::CreateView::new(&name, &view_query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    let support = if writable {
        ViewDeleteSupport::deletable(returning)
    } else {
        ViewDeleteSupport::read_only()
    };
    context.set_view_delete_support(&name, &support);
    let result = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context);
    assert_eq!(
        context.reported_problem_count() == 0,
        valid,
        "{:?}",
        context.reported_problems()
    );
    if valid {
        assert_eq!(
            result.unwrap().as_delete().unwrap().target().to_string(),
            "public.writable_items"
        );
    }
}
