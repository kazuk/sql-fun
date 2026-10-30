use crate::ast::Dialect;
use crate::sem::{ColumnName, FullName, ViewLike, ViewUpdateSupport};

#[rstest::rstest]
#[case::writable("UPDATE writable_items SET id=$1 RETURNING *", true, true, true)]
#[case::cte_shadow(
    "WITH writable_items AS (SELECT 2 other) UPDATE writable_items SET id=1 RETURNING *",
    true,
    true,
    true
)]
#[case::read_only("UPDATE writable_items SET id=1", false, false, false)]
#[case::column("UPDATE writable_items SET label='one'", true, true, false)]
#[case::returning("UPDATE writable_items SET id=1 RETURNING id", true, false, false)]
fn respects_update_capabilities(
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
        .resolve(&mut context, &mut crate::CacheContext::new())
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "writable_items");
    let view = crate::sem::CreateView::new(&name, &view_query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    let support = if writable {
        ViewUpdateSupport::writable(
            &[view.get_column_def(&ColumnName::from("id")).unwrap()],
            returning,
        )
    } else {
        ViewUpdateSupport::read_only()
    };
    context.set_view_update_support(&name, &support);
    let result = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context, &mut crate::CacheContext::new());
    assert_eq!(
        context.reported_problem_count() == 0,
        valid,
        "{:?}",
        context.reported_problems()
    );
    if valid {
        assert_eq!(
            result.unwrap().as_update().unwrap().target().to_string(),
            "public.writable_items"
        );
    }
}
