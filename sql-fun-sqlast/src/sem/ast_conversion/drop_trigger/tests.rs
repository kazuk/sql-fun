use crate::sem::SemAst;
use crate::test_helpers::heavy::drop_trigger as heavy;
use crate::{
    context::{ParseContext, TriggerInfoRead},
    names::{FullName, SchemaName},
    sem::TriggerIdentity,
    test_helpers::create_cache_context as test_helpers_create_cache_context,
};

#[rstest::rstest]
#[case::restrict("DROP TRIGGER audit ON item_view RESTRICT", false)]
#[case::cascade("DROP TRIGGER audit ON public.item_view CASCADE", true)]
fn resolved_drop_does_not_apply_until_requested(
    #[case] sql: &str,
    #[case] cascade: bool,
) -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let mut child = test_helpers::layer(&parent);
    let statement = SemAst::from_ast_node(
        &test_helpers::statement(sql).node,
        &mut child,
        &mut test_helpers_create_cache_context(),
    )?;
    let drop = statement.trigger_drop().unwrap();
    assert!(drop.found());
    assert_eq!(drop.identity(), Some(&test_helpers::identity()));
    assert_eq!(drop.behavior().unwrap().is_cascade(), cascade);
    assert!(!drop.if_exists());
    assert!(child.trigger(&test_helpers::identity()).is_some());
    Ok(())
}

#[test]
fn drop_masks_inherited_definition_and_recreation_replaces_the_tombstone() -> testresult::TestResult
{
    let parent = heavy::catalog()?;
    let child = test_helpers::apply(
        test_helpers::layer(&parent),
        "DROP TRIGGER audit ON item_view",
    )?
    .into_context()
    .freeze();
    assert!(child.trigger(&test_helpers::identity()).is_none());
    assert!(
        test_helpers::layer(&child)
            .trigger(&test_helpers::identity())
            .is_none()
    );
    assert!(parent.trigger(&test_helpers::identity()).is_some());
    assert!(
        test_helpers::layer(&parent)
            .trigger(&test_helpers::identity())
            .is_some()
    );
    assert_eq!(test_helpers::active_count(&parent), 2);
    assert_eq!(test_helpers::active_count(&child), 1);
    let grandchild = test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
        .into_context()
        .freeze();
    assert!(grandchild.trigger(&test_helpers::identity()).is_some());
    assert_eq!(test_helpers::active_count(&grandchild), 2);
    assert!(child.trigger(&test_helpers::identity()).is_none());
    Ok(())
}

#[rstest::rstest]
#[case::missing_trigger("DROP TRIGGER IF EXISTS missing ON item_view", true)]
#[case::missing_relation("DROP TRIGGER IF EXISTS audit ON absent", false)]
fn if_exists_retains_completed_absence_without_changing_associations(
    #[case] sql: &str,
    #[case] relation_found: bool,
) -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let pair = test_helpers::apply(test_helpers::layer(&parent), sql)?;
    let drop = pair.ast().trigger_drop().unwrap();
    assert!(!drop.found());
    assert!(drop.if_exists());
    assert_eq!(drop.identity().is_some(), relation_found);
    assert_eq!(test_helpers::active_count(&pair.into_context().freeze()), 2);
    Ok(())
}

#[rstest::rstest]
#[case::missing_trigger("DROP TRIGGER missing ON item_view")]
#[case::missing_relation("DROP TRIGGER audit ON absent")]
fn invalid_drop_leaves_named_definitions_and_links_intact(
    #[case] sql: &str,
) -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let mut child = test_helpers::layer(&parent);
    assert!(
        SemAst::from_ast_node(
            &test_helpers::statement(sql).node,
            &mut child,
            &mut crate::test_helpers::create_cache_context()
        )
        .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement))
    );
    assert!(child.trigger(&test_helpers::identity()).is_some());
    assert_eq!(test_helpers::active_count(&child.freeze()), 2);
    Ok(())
}

#[test]
fn relation_namespace_and_quoted_trigger_names_remain_distinct() -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let context = test_helpers::apply(test_helpers::layer(&parent),
        r#"CREATE TRIGGER "Audit.Name" BEFORE INSERT ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()"#)?.into_context();
    let context = test_helpers::apply(
        context,
        "CREATE TRIGGER audit BEFORE INSERT ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()",
    )?
    .into_context();
    let pair = test_helpers::apply(
        context,
        r#"DROP TRIGGER "Audit.Name" ON public.items CASCADE"#,
    )?;
    let quoted = TriggerIdentity::postgres(
        &FullName::with_schema("public", "items"),
        &FullName::with_schema("public", "Audit.Name"),
    );
    let context = pair.into_context().freeze();
    assert!(context.trigger(&quoted).is_none());
    assert!(context.trigger(&test_helpers::identity()).is_some());
    let table_identity = TriggerIdentity::postgres(
        &FullName::with_schema("public", "items"),
        &FullName::with_schema("public", "audit"),
    );
    assert!(context.trigger(&table_identity).is_some());
    assert_eq!(test_helpers::table_trigger_count(&context), 1);
    Ok(())
}

#[test]
fn relation_search_path_does_not_fall_through_to_a_later_trigger() -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let mut context = test_helpers::apply(
        test_helpers::layer(&parent),
        "CREATE TABLE earlier.item_view(id integer)",
    )?
    .into_context();
    context.set_search_path(&[SchemaName::from("earlier"), SchemaName::from("public")]);
    let pair = test_helpers::apply(context, "DROP TRIGGER IF EXISTS audit ON item_view")?;
    let drop = pair.ast().trigger_drop().unwrap();
    assert!(!drop.found());
    assert_eq!(
        drop.identity().unwrap().relation(),
        Some(&FullName::with_schema("earlier", "item_view"))
    );
    assert!(
        pair.into_context()
            .trigger(&test_helpers::identity())
            .is_some()
    );
    Ok(())
}

use crate::test_helpers::drop_trigger as test_helpers;

#[cfg(feature = "sqlite")]
mod sqlite;
