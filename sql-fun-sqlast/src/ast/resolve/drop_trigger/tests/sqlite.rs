use crate::test_helpers::heavy::drop_trigger_sqlite as heavy;
use crate::{
    context::{ParseContext, TriggerInfoRead},
    names::FullName,
    sem::TriggerIdentity,
};

#[test]
fn drop_masks_inherited_trigger_and_recreation_preserves_other_links() -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let pair = test_helpers::apply(test_helpers::layer(&parent), "DROP TRIGGER AUDIT")?;
    let drop = pair.ast().trigger_drop().unwrap();
    assert_eq!(drop.identity(), Some(&test_helpers::identity()));
    assert_eq!(
        drop.target(),
        Some(&FullName::with_schema("main", "item_view"))
    );
    assert!(drop.requested_target().is_none());
    assert!(drop.behavior().is_none());
    let child = pair.into_context().freeze();
    assert!(child.trigger(&test_helpers::identity()).is_none());
    assert!(
        test_helpers::layer(&child)
            .trigger(&test_helpers::identity())
            .is_none()
    );
    assert_eq!(test_helpers::active_count(&child), 1);
    assert_eq!(test_helpers::active_count(&parent), 2);
    assert!(
        test_helpers::layer(&parent)
            .trigger(&test_helpers::identity())
            .is_some()
    );
    let recreated = test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
        .into_context()
        .freeze();
    assert_eq!(test_helpers::active_count(&recreated), 2);
    assert!(recreated.trigger(&test_helpers::identity()).is_some());
    Ok(())
}

#[rstest::rstest]
#[case("DROP TRIGGER IF EXISTS absent")]
#[case("DROP TRIGGER IF EXISTS main.absent")]
fn absent_if_exists_is_an_explicit_noop(#[case] sql: &str) -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let pair = test_helpers::apply(test_helpers::layer(&parent), sql)?;
    let drop = pair.ast().trigger_drop().unwrap();
    assert!(!drop.found());
    assert!(drop.if_exists());
    assert!(drop.target().is_none());
    assert_eq!(test_helpers::active_count(&pair.into_context().freeze()), 2);
    let mut context = test_helpers::layer(&parent);
    assert!(
        test_helpers::statement("DROP TRIGGER absent")
            .node
            .resolve(
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement))
    );
    assert_eq!(test_helpers::active_count(&context.freeze()), 2);
    Ok(())
}

#[test]
fn temp_trigger_precedes_main_and_qualified_drop_selects_its_namespace() -> testresult::TestResult {
    let parent = heavy::catalog()?;
    let context = test_helpers::apply(
        test_helpers::layer(&parent),
        "CREATE TEMP TRIGGER audit INSTEAD OF INSERT ON main.item_view BEGIN SELECT 1; END",
    )?
    .into_context();
    let pair = test_helpers::apply(context, "DROP TRIGGER audit")?;
    let temp = TriggerIdentity::sqlite(&FullName::with_schema("temp", "audit"));
    assert_eq!(pair.ast().trigger_drop().unwrap().identity(), Some(&temp));
    let context = pair.into_context();
    assert!(context.trigger(&test_helpers::identity()).is_some());
    assert!(context.trigger(&temp).is_none());
    let pair = test_helpers::apply(context, "DROP TRIGGER MAIN.AuDiT")?;
    assert_eq!(
        pair.ast().trigger_drop().unwrap().identity(),
        Some(&test_helpers::identity())
    );
    assert!(
        pair.into_context()
            .trigger(&test_helpers::identity())
            .is_none()
    );
    Ok(())
}

#[test]
fn agrees_with_bundled_sqlite_drop_namespace_behavior() -> testresult::TestResult {
    let connection = ::rusqlite::Connection::open_in_memory()?;
    connection.execute_batch("CREATE TABLE items(id); CREATE VIEW item_view AS SELECT id FROM items; CREATE TRIGGER audit INSTEAD OF INSERT ON item_view BEGIN SELECT 1; END; CREATE TEMP TRIGGER audit INSTEAD OF INSERT ON main.item_view BEGIN SELECT 1; END; DROP TRIGGER AuDiT;")?;
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type='trigger' AND name='audit'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(count, 1);
    connection.execute_batch("DROP TRIGGER MAIN.AUDIT; DROP TRIGGER IF EXISTS absent;")?;
    assert!(connection.execute_batch("DROP TRIGGER absent").is_err());
    Ok(())
}

#[rstest::rstest]
#[case::first_attached("zeta", "alpha", false)]
#[case::reversed_attachments("alpha", "zeta", false)]
#[case::qualified_later_attachment("zeta", "alpha", true)]
fn attached_schema_order_matches_bundled_sqlite(
    #[case] first: &str,
    #[case] second: &str,
    #[case] qualified: bool,
) -> testresult::TestResult {
    let (parent, connection) = heavy::attached_catalog(first, second)?;
    let selected = if qualified { second } else { first };
    let sql = if qualified {
        format!("DROP TRIGGER {second}.audit")
    } else {
        "DROP TRIGGER audit".to_owned()
    };
    let pair = test_helpers::apply(test_helpers::layer(&parent), &sql)?;
    let selected_identity = TriggerIdentity::sqlite(&FullName::with_schema(selected, "audit"));
    let drop = pair.ast().trigger_drop().unwrap();
    assert_eq!(drop.identity(), Some(&selected_identity));
    assert_eq!(
        drop.target(),
        Some(&FullName::with_schema(selected, "items"))
    );
    connection.execute_batch(&sql)?;
    let child = pair.into_context().freeze();
    for schema in [first, second] {
        let count: i64 = connection.query_row(
            &format!(
                "SELECT count(*) FROM {schema}.sqlite_schema WHERE type='trigger' AND name='audit'"
            ),
            [],
            |row| row.get(0),
        )?;
        let identity = TriggerIdentity::sqlite(&FullName::with_schema(schema, "audit"));
        assert_eq!(count, i64::from(schema != selected));
        assert_eq!(child.trigger(&identity).is_some(), count == 1);
        assert!(parent.trigger(&identity).is_some());
        assert_eq!(
            test_helpers::table_trigger_count(&child, schema),
            usize::from(schema != selected)
        );
        assert_eq!(test_helpers::table_trigger_count(&parent, schema), 1);
    }
    Ok(())
}

use crate::test_helpers::drop_trigger_sqlite as test_helpers;
