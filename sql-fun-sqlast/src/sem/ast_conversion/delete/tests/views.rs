use crate::ast::Dialect;
use crate::names::FullName;
use crate::parse_statements;
use crate::sem::CreateView;
use crate::sem::SemAst;
use crate::sem::ViewDeleteSupport;
use crate::sem::ViewLike;
use crate::test_helpers::create_cache_context;
use crate::test_helpers::delete::catalog;

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
    let mut context = catalog(Dialect::postgres());
    let view_query = SemAst::from_ast_node(
        &parse_statements("SELECT id,label FROM items", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node,
        &mut context,
        &mut create_cache_context(),
    )
    .unwrap()
    .into_select_statemtnt()
    .unwrap();
    let name = FullName::with_schema("public", "writable_items");
    let view = CreateView::new(&name, &view_query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(ViewLike::from(&view)));
    let support = if writable {
        ViewDeleteSupport::deletable(returning)
    } else {
        ViewDeleteSupport::read_only()
    };
    context.set_view_delete_support(&name, &support);
    let result = SemAst::from_ast_node(
        &parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node,
        &mut context,
        &mut create_cache_context(),
    );
    assert_eq!(
        context.reported_problem_count() == 0,
        valid,
        "{:?}",
        context.reported_problems()
    );
    let result = result.unwrap();
    assert!(result.as_delete().unwrap().target_found());
    assert_eq!(result.as_delete().unwrap().view_support(), Some(&support));
    if valid {
        assert_eq!(
            result.as_delete().unwrap().target().to_string(),
            "public.writable_items"
        );
    }
}
