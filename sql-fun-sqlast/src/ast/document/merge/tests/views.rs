use crate::ast::Dialect;
use crate::sem::{
    ColumnDefinition, ColumnName, CreateTableAs, CreateView, FullName, PgBuiltInType, SemAst,
    ViewLike, ViewMergeSupport, ViewMergeTriggers,
};
use crate::test_helpers::TestParseContext;

use super::test_helpers::{catalog, resolve};

const VIEW_NAME: &str = "writable_items";

#[test]
fn default_values_requires_insert_capability_but_no_assignable_columns() {
    let support = ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), Some(&[]), None);
    let mut context = view_catalog(17, &support);
    let sql = "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES";
    resolve(sql, &mut context).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let support = ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), None, None);
    let mut context = view_catalog(17, &support);
    assert!(resolve(sql, &mut context).is_err());
}

#[test]
fn automatic_view_requires_postgresql_17_and_preserves_returning_support() {
    let support = automatic_support();
    let mut context = view_catalog(17, &support);
    let statement = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE RETURNING t.id",
        &mut context,
    )
    .unwrap();
    let merge = statement.as_merge().unwrap();

    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(merge.result_columns().len(), 1);
    assert_eq!(merge.view_support(), Some(&support));

    let mut context = catalog(16);
    install_view(&mut context, false);
    let result = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        &mut context,
    );
    assert!(result.is_err() || context.reported_problem_count() > 0);
}

#[test]
fn trigger_backed_view_requires_a_trigger_for_each_action_type() {
    let support = trigger_support(true, false, true);
    let mut context = view_catalog(17, &support);
    let accepted = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel)",
        &mut context,
    );
    assert!(
        accepted.is_ok(),
        "{accepted:?} {:?}",
        context.reported_problems()
    );

    let mut context = view_catalog(17, &support);
    let rejected = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel WHEN NOT MATCHED BY SOURCE THEN DELETE",
        &mut context,
    );
    assert!(rejected.is_err() || context.reported_problem_count() > 0);

    let all_actions = trigger_support(true, true, true);
    let mut context = view_catalog(17, &all_actions);
    let accepted = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel WHEN NOT MATCHED THEN INSERT(id,label) VALUES(s.sid,s.slabel) WHEN NOT MATCHED BY SOURCE THEN DELETE",
        &mut context,
    );
    assert!(
        accepted.is_ok(),
        "{accepted:?} {:?}",
        context.reported_problems()
    );
}

#[test]
fn rules_and_materialized_views_are_rejected() {
    let with_rules = ViewMergeSupport::new(
        true,
        true,
        ViewMergeTriggers::new(true, true, true),
        Some(&all_columns()),
        Some(&all_columns()),
    );
    let mut context = view_catalog(17, &with_rules);
    let result = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        &mut context,
    );
    assert!(result.is_err() || context.reported_problem_count() > 0);

    let mut context = catalog(17);
    install_view(&mut context, true);
    let result = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN DELETE",
        &mut context,
    );
    assert!(result.is_err() || context.reported_problem_count() > 0);
}

#[test]
fn insert_and_update_assignments_use_their_action_specific_writable_columns() {
    let support = ViewMergeSupport::new(
        true,
        false,
        ViewMergeTriggers::default(),
        Some(&columns(&["id"])),
        Some(&columns(&["label"])),
    );

    let mut context = view_catalog(17, &support);
    let insert = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)",
        &mut context,
    );
    assert!(
        insert.is_ok(),
        "{insert:?} {:?}",
        context.reported_problems()
    );

    let mut context = view_catalog(17, &support);
    let insert_read_only = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(label) VALUES(s.slabel)",
        &mut context,
    );
    assert!(insert_read_only.is_err() || context.reported_problem_count() > 0);

    let mut context = view_catalog(17, &support);
    let update = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel",
        &mut context,
    );
    assert!(
        update.is_ok(),
        "{update:?} {:?}",
        context.reported_problems()
    );

    let mut context = view_catalog(17, &support);
    let update_read_only = resolve(
        "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid",
        &mut context,
    );
    assert!(update_read_only.is_err() || context.reported_problem_count() > 0);
}

fn view_catalog(version: u32, support: &ViewMergeSupport) -> TestParseContext {
    let mut context = catalog(version);
    install_view(&mut context, false);
    context.set_view_merge_support(&view_name(), support);
    context
}

fn install_view(context: &mut TestParseContext, materialized: bool) {
    let query = crate::parse_statements("SELECT id,label FROM items", Dialect::postgres())
        .next()
        .unwrap()
        .unwrap()
        .node
        .resolve(context, &mut crate::test_helpers::create_cache_context())
        .unwrap()
        .into_select_statemtnt()
        .unwrap();
    let name = view_name();
    let view = if materialized {
        let definition = CreateTableAs::new(&name, &SemAst::select(&query), true);
        ViewLike::from(&definition)
    } else {
        let definition = CreateView::new(&name, &query, &[], &None, false);
        ViewLike::from(&definition)
    };
    context.set_get_view_impl_result(&name, &Some(view.clone()));
}

fn view_name() -> FullName {
    FullName::with_schema("public", VIEW_NAME)
}

fn automatic_support() -> ViewMergeSupport {
    ViewMergeSupport::new(
        true,
        false,
        ViewMergeTriggers::default(),
        Some(&all_columns()),
        Some(&all_columns()),
    )
}

fn trigger_support(insert: bool, update: bool, delete: bool) -> ViewMergeSupport {
    ViewMergeSupport::new(
        false,
        false,
        ViewMergeTriggers::new(insert, update, delete),
        Some(&all_columns()),
        Some(&all_columns()),
    )
}

fn all_columns() -> Vec<ColumnDefinition> {
    columns(&["id", "label"])
}

fn columns(names: &[&str]) -> Vec<ColumnDefinition> {
    names
        .iter()
        .map(|name| {
            let type_ref = if *name == "id" {
                PgBuiltInType::int4()
            } else {
                PgBuiltInType::text()
            };
            ColumnDefinition::new(
                &Some(ColumnName::from(*name)),
                Some(&type_ref),
                Some(true),
                &[],
            )
        })
        .collect()
}
