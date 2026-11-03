use crate::{
    ast::{Dialect, resolve::update::tests::test_helpers::catalog},
    names::{ColumnName, FullName},
    parse_statements,
    sem::{CreateView, ViewLike, ViewUpdateSupport},
    test_helpers::create_cache_context,
};

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
    let mut context = catalog(Dialect::postgres());
    let view_query = parse_statements("SELECT id,label FROM items", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context, &mut create_cache_context())
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "writable_items");
    let view = CreateView::new(&name, &view_query, &[], &None, false);
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
    let result = parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(&mut context, &mut create_cache_context());
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
