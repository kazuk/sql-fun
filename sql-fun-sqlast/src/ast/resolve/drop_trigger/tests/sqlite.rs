use crate::sem::{BaseParseContext, FullName, ParseContext, TriggerIdentity};

#[test]
fn drop_masks_inherited_trigger_and_recreation_preserves_other_links() -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
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
    let parent = test_helpers::catalog()?;
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
            .resolve(&mut context)
            .is_err()
    );
    assert_eq!(test_helpers::active_count(&context.freeze()), 2);
    Ok(())
}

#[test]
fn temp_trigger_precedes_main_and_qualified_drop_selects_its_namespace() -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
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

mod test_helpers {
    use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
    use crate::sem::{BaseParseContext, FullName, ParseContext, TriggerIdentity, ViewInfoRead};
    pub(super) const CREATE_AUDIT: &str =
        "CREATE TRIGGER audit INSTEAD OF INSERT ON item_view BEGIN SELECT 1; END";
    pub(super) fn identity() -> TriggerIdentity {
        TriggerIdentity::sqlite(&FullName::with_schema("main", "audit"))
    }
    // Small explicitly selected builtin fixture; no full production master is loaded.
    pub(super) fn catalog() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
        use ::clap::Parser;
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let args = ::sql_fun_core::SqlFunArgs::try_parse_from([
            "sql-fun",
            "--metadata-file",
            manifest
                .join("tests/fixtures/writable-sqlite.toml")
                .to_str()
                .unwrap(),
            "--sql-fun-home",
            manifest
                .join("tests/fixtures/writable-builtins")
                .to_str()
                .unwrap(),
            "--postgres-version",
            "17",
        ])?;
        let metadata = args.load_metadata()?.get("default").unwrap().clone();
        let base = FrozenBaseContext::from(BaseContext::new(&args, &metadata)?);
        let mut context = SchemaFileContext::new(
            std::path::Path::new("drop-trigger.sql"),
            &base,
            &args,
            &metadata,
        );
        for sql in [
            "CREATE TABLE items(id)",
            "CREATE VIEW item_view AS SELECT id FROM items",
            CREATE_AUDIT,
            "CREATE TRIGGER other INSTEAD OF UPDATE ON item_view BEGIN SELECT 1; END",
        ] {
            context = apply(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(context.freeze())
    }
    pub(super) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("drop-trigger.sql"),
            base,
            base.get_args(),
            base.metadata(),
        )
    }
    pub(super) fn statement(sql: &str) -> crate::ast::Statement {
        crate::parse_statements(sql, crate::ast::Dialect::sqlite())
            .next()
            .unwrap()
            .expect("valid test SQL")
    }
    pub(super) fn apply(
        mut context: SchemaFileContext,
        sql: &str,
    ) -> Result<crate::AstAndContextPair<SchemaFileContext>, crate::sem::AnalysisError> {
        crate::sem::MutSourcePathWithSpan::set_source_path_with_span(
            &mut context,
            &crate::test_helpers::source_span(),
        );
        statement(sql).analyze(context)
    }
    pub(super) fn active_count(context: &FrozenBaseContext) -> usize {
        let target = FullName::with_schema("main", "item_view");
        context
            .get_view_impl(&target)
            .unwrap()
            .triggers()
            .unwrap()
            .resolve(context, &target)
            .len()
    }
}
