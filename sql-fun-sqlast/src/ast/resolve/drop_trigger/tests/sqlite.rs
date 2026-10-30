use crate::{
    context::{ParseContext, TriggerInfoRead},
    sem::{FullName, TriggerIdentity},
};

#[test]
fn drop_masks_inherited_trigger_and_recreation_preserves_other_links() -> testresult::TestResult {
    let parent = test_helpers::heavy::catalog()?;
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
    let parent = test_helpers::heavy::catalog()?;
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
    let parent = test_helpers::heavy::catalog()?;
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
    let (parent, connection) = test_helpers::heavy::attached_catalog(first, second)?;
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

mod test_helpers {
    use crate::{
        context::{BaseParseContext, FrozenBaseContext, SchemaFileContext},
        sem::{FullName, SchemaName, TableName, TriggerIdentity, ViewName},
    };
    pub(super) const CREATE_AUDIT: &str =
        "CREATE TRIGGER audit INSTEAD OF INSERT ON item_view BEGIN SELECT 1; END";
    pub(super) fn identity() -> TriggerIdentity {
        TriggerIdentity::sqlite(&FullName::with_schema("main", "audit"))
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
        let mut cache_context = crate::test_helpers::create_cache_context();
        crate::context::MutSourcePathWithSpan::set_source_path_with_span(
            &mut context,
            &crate::test_helpers::source_span(),
        );
        statement(sql).analyze(context, &mut cache_context)
    }
    pub(super) fn table_trigger_count(context: &FrozenBaseContext, schema: &str) -> usize {
        let target = FullName::with_schema(schema, "items");
        context
            .get_table(&TableName::from(target.clone()))
            .unwrap()
            .triggers()
            .resolve(context, &target)
            .len()
    }
    pub(super) fn active_count(context: &FrozenBaseContext) -> usize {
        let target = FullName::with_schema("main", "item_view");
        context
            .get_view(&ViewName::from_full_name(&target))
            .unwrap()
            .triggers()
            .unwrap()
            .resolve(context, &target)
            .len()
    }
    pub(super) mod heavy {
        use super::{CREATE_AUDIT, FrozenBaseContext, SchemaFileContext, SchemaName, apply, layer};
        use crate::context::{BaseContext, ParseContext};

        /// Reload the maintained catalog and analyze the test schema on each call.
        /// Catalog state is not shared across independent test processes.
        pub(in super::super) fn catalog() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            use ::clap::Parser;
            let args = ::sql_fun_core::SqlFunArgs::try_parse_from([
                "sql-fun",
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                "17",
            ])?;
            let metadata = ::serde_json::from_value(::serde_json::json!({
                "engine": "SQLite", "dialect": "SQLite", "version": "3",
                "search_path": ["main"]
            }))?;
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

        /// Load the maintained catalog and an in-memory SQLite oracle per call.
        /// Neither catalog nor connection is shared across test processes.
        pub(in super::super) fn attached_catalog(
            first: &str,
            second: &str,
        ) -> Result<(FrozenBaseContext, ::rusqlite::Connection), Box<dyn std::error::Error>>
        {
            let base = catalog()?;
            let mut context = apply(layer(&base), "DROP TRIGGER main.audit")?.into_context();
            context.set_search_path(&[
                SchemaName::from("main"),
                SchemaName::from(first),
                SchemaName::from(second),
            ]);
            let connection = ::rusqlite::Connection::open_in_memory()?;
            for schema in [first, second] {
                connection.execute_batch(&format!("ATTACH DATABASE ':memory:' AS {schema}"))?;
                for sql in [
                    format!("CREATE TABLE {schema}.items(id)"),
                    format!(
                        "CREATE TRIGGER {schema}.audit BEFORE INSERT ON {schema}.items BEGIN SELECT 1; END"
                    ),
                ] {
                    connection.execute_batch(&sql)?;
                    context = apply(context, &sql)?.into_context();
                }
            }
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            Ok((context.freeze(), connection))
        }
    }
}
