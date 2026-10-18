use crate::ast::Dialect;
use crate::sem::{BaseParseContext, FullName, ParseContext, TableInfoRead, TriggerIdentity};

#[rstest::rstest]
#[case::function_when(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (predicate_fn()) EXECUTE FUNCTION audit_fn()"
)]
#[case::null_when(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::string_when(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN ('true') EXECUTE FUNCTION audit_fn()"
)]
#[case::row(
    "CREATE TRIGGER audit BEFORE UPDATE OF id ON items FOR EACH ROW WHEN (NEW.id IS NOT NULL) EXECUTE FUNCTION audit_fn('arg', 12)"
)]
#[case::statement("CREATE TRIGGER audit AFTER INSERT ON items EXECUTE FUNCTION audit_fn()")]
#[case::transition(
    "CREATE TRIGGER audit AFTER UPDATE ON items REFERENCING OLD TABLE AS previous NEW TABLE AS current FOR EACH STATEMENT EXECUTE FUNCTION audit_fn()"
)]
#[case::constraint(
    "CREATE CONSTRAINT TRIGGER audit AFTER UPDATE ON items DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION audit_fn()"
)]
fn resolves_postgres_statement_without_applying(#[case] sql: &str) -> testresult::TestResult {
    let mut context = test_helpers::catalog(Dialect::postgres());
    let statement = test_helpers::resolve(sql, &mut context)?;
    let trigger = statement.trigger().unwrap();
    assert_eq!(trigger.target(), &FullName::with_schema("public", "items"));
    assert!(context.trigger(trigger.identity()).is_none());
    assert!(
        context
            .get_table_impl(trigger.target())
            .unwrap()
            .triggers()
            .names()
            .is_empty()
    );
    let function = trigger.body().function_reference().unwrap();
    assert_eq!(
        function.name(),
        &FullName::with_schema("public", "audit_fn")
    );
    if sql.contains("'arg'") {
        assert_eq!(function.arguments(), &["arg", "12"]);
    }
    if let Some(predicate) = trigger.predicate() {
        assert_eq!(
            predicate.get_type(),
            Some(crate::sem::PgBuiltInType::bool())
        );
    }
    if sql.contains("REFERENCING") {
        assert_eq!(trigger.transitions().len(), 2);
    }
    Ok(())
}

#[rstest::rstest]
#[case::set_returning_function(
    "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION set_trigger_fn()"
)]
#[case::aggregate_when(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (aggregate_fn()) EXECUTE FUNCTION audit_fn()"
)]
#[case::unknown_target("CREATE TRIGGER audit BEFORE INSERT ON absent EXECUTE FUNCTION audit_fn()")]
#[case::unknown_column(
    "CREATE TRIGGER audit BEFORE UPDATE OF absent ON items EXECUTE FUNCTION audit_fn()"
)]
#[case::unknown_function("CREATE TRIGGER audit BEFORE INSERT ON items EXECUTE FUNCTION absent()")]
#[case::old_insert(
    "CREATE TRIGGER audit BEFORE INSERT ON items FOR EACH ROW WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::new_delete(
    "CREATE TRIGGER audit BEFORE DELETE ON items FOR EACH ROW WHEN (NEW.id IS NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::mixed_old(
    "CREATE TRIGGER audit BEFORE INSERT OR UPDATE ON items FOR EACH ROW WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::unqualified(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (id IS NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::statement_old(
    "CREATE TRIGGER audit BEFORE UPDATE ON items WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
)]
#[case::instead_table(
    "CREATE TRIGGER audit INSTEAD OF UPDATE ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()"
)]
#[case::truncate_row(
    "CREATE TRIGGER audit AFTER TRUNCATE ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()"
)]
#[case::transition_columns(
    "CREATE TRIGGER audit AFTER UPDATE OF id ON items REFERENCING OLD TABLE AS previous EXECUTE FUNCTION audit_fn()"
)]
#[case::transition_event(
    "CREATE TRIGGER audit AFTER INSERT ON items REFERENCING OLD TABLE AS previous EXECUTE FUNCTION audit_fn()"
)]
#[case::not_boolean(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (NEW.id) EXECUTE FUNCTION audit_fn()"
)]
#[case::subquery(
    "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (EXISTS(SELECT 1)) EXECUTE FUNCTION audit_fn()"
)]
fn rejects_invalid_postgres_semantics(#[case] sql: &str) {
    let mut context = test_helpers::catalog(Dialect::postgres());
    assert!(test_helpers::resolve(sql, &mut context).is_err(), "{sql}");
    assert!(context.reported_problem_count() > 0);
    let key = TriggerIdentity::postgres(
        &FullName::with_schema("public", "items"),
        &FullName::with_schema("public", "audit"),
    );
    assert!(context.trigger(&key).is_none());
}

#[test]
fn named_lookup_and_target_links_agree_across_layers() -> testresult::TestResult {
    let root = test_helpers::catalog(Dialect::postgres()).freeze();
    let parent = test_helpers::layer(&root);
    let parent = test_helpers::apply(
        "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION audit_fn()",
        parent,
    )?
    .into_context();
    let parent = parent.freeze();
    let sibling = test_helpers::layer(&parent);
    let child = test_helpers::apply(
        "CREATE TRIGGER second BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn()",
        test_helpers::layer(&parent),
    )?
    .into_context();
    let target = FullName::with_schema("public", "items");
    let names = child.get_table_impl(&target).unwrap().triggers();
    assert_eq!(names.names().len(), 2);
    let definitions = names.resolve(&child, &target);
    assert_eq!(definitions.len(), 2);
    for definition in definitions {
        assert!(std::ptr::eq(
            definition,
            child.trigger(definition.identity()).unwrap()
        ));
    }
    assert_eq!(
        parent
            .get_table_impl(&target)
            .unwrap()
            .triggers()
            .names()
            .len(),
        1
    );
    assert_eq!(
        sibling
            .get_table_impl(&target)
            .unwrap()
            .triggers()
            .names()
            .len(),
        1
    );
    let child = test_helpers::apply("CREATE OR REPLACE TRIGGER audit BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn('changed')", child)?.into_context();
    assert_eq!(
        child
            .get_table_impl(&target)
            .unwrap()
            .triggers()
            .names()
            .len(),
        2
    );
    let identity = TriggerIdentity::postgres(&target, &FullName::with_schema("public", "audit"));
    assert_eq!(
        child
            .trigger(&identity)
            .unwrap()
            .body()
            .function_reference()
            .unwrap()
            .arguments(),
        &["changed"]
    );
    assert!(
        parent
            .trigger(&identity)
            .unwrap()
            .body()
            .function_reference()
            .unwrap()
            .arguments()
            .is_empty()
    );
    let descendant = test_helpers::layer(&child.freeze());
    assert_eq!(
        descendant
            .get_table_impl(&target)
            .unwrap()
            .triggers()
            .resolve(&descendant, &target)
            .len(),
        2
    );
    assert!(
        test_helpers::apply(
            "CREATE TRIGGER audit BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn()",
            sibling
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn postgres_preserves_quoted_trigger_identity() -> testresult::TestResult {
    let root = test_helpers::catalog(Dialect::postgres()).freeze();
    let context = test_helpers::apply(
        r#"CREATE TRIGGER "Audit.With.Dot" AFTER UPDATE ON public.items EXECUTE FUNCTION public.audit_fn()"#,
        test_helpers::layer(&root),
    )?.into_context();
    let target = FullName::with_schema("public", "items");
    let name = FullName::with_schema("public", "Audit.With.Dot");
    let identity = TriggerIdentity::postgres(&target, &name);
    assert_eq!(context.trigger(&identity).unwrap().identity().name(), &name);
    assert_eq!(
        context.get_table_impl(&target).unwrap().triggers().names(),
        &[name]
    );
    assert!(
        context
            .trigger(&TriggerIdentity::postgres(
                &target,
                &FullName::with_schema("public", "audit.with.dot"),
            ))
            .is_none()
    );
    Ok(())
}

#[test]
fn postgres_names_are_scoped_to_their_relations() -> testresult::TestResult {
    let root = test_helpers::catalog(Dialect::postgres()).freeze();
    let context = test_helpers::apply(
        "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION audit_fn()",
        test_helpers::layer(&root),
    )?
    .into_context();
    let context = test_helpers::apply(
        "CREATE TRIGGER audit AFTER UPDATE ON other EXECUTE FUNCTION audit_fn()",
        context,
    )?
    .into_context();
    for target in ["items", "other"] {
        let relation = FullName::with_schema("public", target);
        let key = TriggerIdentity::postgres(&relation, &FullName::with_schema("public", "audit"));
        assert_eq!(context.trigger(&key).unwrap().target(), &relation);
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::select(
    "CREATE TRIGGER audit AFTER UPDATE ON items WHEN NEW.id IS NOT NULL BEGIN SELECT OLD.id, NEW.id; END"
)]
#[case::target_case("CREATE TRIGGER audit AFTER UPDATE ON ITEMS BEGIN SELECT NEW.ID; END")]
#[case::case_insensitive(
    "CREATE TRIGGER audit AFTER UPDATE OF ID ON items BEGIN SELECT NEW.ID; END"
)]
#[case::nested("CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT (SELECT NEW.id); END")]
#[case::insert(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN INSERT INTO other(id) VALUES(NEW.id); END"
)]
#[case::update(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN UPDATE other SET id = NEW.id WHERE OLD.id IS NOT NULL; END"
)]
#[case::delete(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN DELETE FROM other WHERE OLD.id IS NOT NULL; END"
)]
#[case::unknown_event_column(
    "CREATE TRIGGER audit AFTER UPDATE OF missing ON items BEGIN SELECT NEW.id; END"
)]
fn resolves_sqlite_body(#[case] sql: &str) -> testresult::TestResult {
    let oracle = test_helpers::sqlite_oracle()?;
    oracle.execute_batch(sql)?;
    let mut context = test_helpers::catalog(Dialect::sqlite());
    let statement = test_helpers::resolve(sql, &mut context)?;
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    let trigger = statement.trigger().unwrap();
    assert_eq!(trigger.body().statements().len(), 1);
    assert_eq!(trigger.target(), &FullName::with_schema("main", "items"));
    if sql.contains("missing") {
        assert!(!trigger.events()[0].columns()[0].found());
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_duplicate_namespace_and_if_not_exists() -> testresult::TestResult {
    let root = test_helpers::catalog(Dialect::sqlite()).freeze();
    let context = test_helpers::apply(
        "CREATE TRIGGER Audit AFTER UPDATE ON items BEGIN SELECT NEW.id; END",
        test_helpers::layer(&root),
    )?
    .into_context();
    let key = TriggerIdentity::sqlite(&FullName::with_schema("main", "audit"));
    assert!(context.trigger(&key).is_some());
    let frozen = context.freeze();
    assert!(
        test_helpers::apply(
            "CREATE TRIGGER AUDIT AFTER UPDATE ON other BEGIN SELECT NEW.id; END",
            test_helpers::layer(&frozen)
        )
        .is_err()
    );
    let context = test_helpers::apply(
        "CREATE TRIGGER IF NOT EXISTS AUDIT AFTER UPDATE ON other BEGIN SELECT NEW.id; END",
        test_helpers::layer(&frozen),
    )?
    .into_context();
    assert_eq!(
        context.trigger(&key).unwrap().target(),
        &FullName::with_schema("main", "items")
    );
    assert!(
        context
            .get_table_impl(&FullName::with_schema("main", "other"))
            .unwrap()
            .triggers()
            .names()
            .is_empty()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::postgres(
    Dialect::postgres(),
    "CREATE TRIGGER audit INSTEAD OF UPDATE ON item_view FOR EACH ROW EXECUTE FUNCTION audit_fn()"
)]
#[case::sqlite(
    Dialect::sqlite(),
    "CREATE TRIGGER audit INSTEAD OF UPDATE ON item_view BEGIN SELECT NEW.id; END"
)]
fn associates_trigger_names_with_views(
    #[case] dialect: Dialect,
    #[case] sql: &str,
) -> testresult::TestResult {
    let root = test_helpers::catalog(dialect).freeze();
    let target = if dialect == Dialect::sqlite() {
        FullName::with_schema("main", "item_view")
    } else {
        FullName::with_schema("public", "item_view")
    };
    let context = test_helpers::apply(sql, test_helpers::layer(&root))?.into_context();
    let view = crate::sem::ViewInfoRead::get_view_impl(&context, &target).unwrap();
    let references = view.triggers().unwrap();
    assert_eq!(references.names().len(), 1);
    assert_eq!(references.resolve(&context, &target)[0].target(), &target);
    assert!(
        crate::sem::ViewInfoRead::get_view_impl(&root, &target)
            .unwrap()
            .triggers()
            .unwrap()
            .names()
            .is_empty()
    );
    Ok(())
}

#[test]
fn failed_child_analysis_preserves_parent_and_sibling_catalogs() -> testresult::TestResult {
    let root = test_helpers::catalog(Dialect::postgres()).freeze();
    let target = FullName::with_schema("public", "items");
    let sibling = test_helpers::layer(&root);
    assert!(test_helpers::apply("CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (missing IS NULL) EXECUTE FUNCTION audit_fn()", test_helpers::layer(&root)).is_err());
    assert!(
        root.get_table_impl(&target)
            .unwrap()
            .triggers()
            .names()
            .is_empty()
    );
    assert!(
        sibling
            .get_table_impl(&target)
            .unwrap()
            .triggers()
            .names()
            .is_empty()
    );
    assert!(
        test_helpers::apply(
            "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION audit_fn()",
            sibling
        )
        .is_ok()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn retains_resolved_old_new_bindings_and_body_dependencies() -> testresult::TestResult {
    let mut context = test_helpers::catalog(Dialect::sqlite());
    let statement = test_helpers::resolve(
        "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT OLD.id, NEW.id; INSERT INTO other(id) VALUES(NEW.id); END",
        &mut context,
    )?;
    let trigger = statement.trigger().unwrap();
    let commands = trigger.body().statements();
    let query = commands[0].clone().into_select_statemtnt().unwrap();
    for (index, alias) in ["old", "new"].into_iter().enumerate() {
        let expression = query
            .result_columns()
            .get_column_at(index)
            .unwrap()
            .expression()
            .unwrap();
        let binding = expression.table_column_reference().unwrap();
        assert_eq!(binding.relation(), &FullName::with_schema("main", "items"));
        assert_eq!(binding.alias().as_str(), alias);
        assert_eq!(
            expression.get_type(),
            Some(crate::sem::PgBuiltInType::int4())
        );
    }
    assert!(
        trigger
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("main", "other"))
    );
    assert!(
        trigger
            .dependencies()
            .types()
            .contains(crate::sem::PgBuiltInType::int4().full_name())
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn temp_trigger_uses_temp_namespace_without_moving_target() -> testresult::TestResult {
    let oracle = test_helpers::sqlite_oracle()?;
    let sql = "CREATE TEMP TRIGGER audit AFTER UPDATE ON main.items BEGIN SELECT NEW.id; END";
    oracle.execute_batch(sql)?;
    let root = test_helpers::catalog(Dialect::sqlite()).freeze();
    let context = test_helpers::apply(sql, test_helpers::layer(&root))?.into_context();
    let identity = TriggerIdentity::sqlite(&FullName::with_schema("temp", "audit"));
    let trigger = context.trigger(&identity).unwrap();
    assert!(trigger.temporary());
    assert_eq!(trigger.target(), &FullName::with_schema("main", "items"));
    let names = context.get_table_impl(trigger.target()).unwrap().triggers();
    assert!(std::ptr::eq(
        names.resolve(&context, trigger.target())[0],
        trigger
    ));
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::read("CREATE TRIGGER aux.audit AFTER UPDATE ON items BEGIN SELECT id FROM other; END")]
#[case::write(
    "CREATE TRIGGER aux.audit AFTER UPDATE ON items BEGIN UPDATE OTHER SET id = NEW.id; END"
)]
fn sqlite_body_uses_trigger_database(#[case] sql: &str) -> testresult::TestResult {
    let oracle = test_helpers::sqlite_oracle()?;
    oracle.execute_batch("ATTACH ':memory:' AS aux; CREATE TABLE aux.items(id INTEGER); CREATE TABLE aux.other(id INTEGER);")?;
    oracle.execute_batch(sql)?;
    let mut context = test_helpers::catalog(Dialect::sqlite());
    test_helpers::add_tables(&mut context, "aux");
    let statement = test_helpers::resolve(sql, &mut context)?;
    let trigger = statement.trigger().unwrap();
    assert_eq!(trigger.target(), &FullName::with_schema("aux", "items"));
    assert!(
        trigger
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("aux", "other"))
    );
    assert!(
        !trigger
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("main", "other"))
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::parameter("CREATE TRIGGER audit AFTER UPDATE ON items WHEN ?1 BEGIN SELECT NEW.id; END")]
#[case::cross_database(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT id FROM aux.other; END"
)]
fn rejects_invalid_sqlite_definition(#[case] sql: &str) -> testresult::TestResult {
    let oracle = test_helpers::sqlite_oracle()?;
    oracle.execute_batch("ATTACH ':memory:' AS aux; CREATE TABLE aux.other(id INTEGER);")?;
    assert!(oracle.execute_batch(sql).is_err());
    let mut context = test_helpers::catalog(Dialect::sqlite());
    test_helpers::add_tables(&mut context, "aux");
    assert!(test_helpers::resolve(sql, &mut context).is_err());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::query_cte(
    "WITH Items AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM ITEMS",
    "Items"
)]
#[case::qualified_table(
    "WITH Items AS (SELECT 7 AS cte_only) SELECT id AS picked FROM main.items",
    "main.items"
)]
#[case::nested_cte_shadow(
    "WITH Items AS (SELECT 7 AS outer_only) SELECT picked FROM (WITH items AS (SELECT 9 AS inner_only) SELECT inner_only AS picked FROM ITEMS) s",
    "items"
)]
#[case::trigger_cte(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT picked FROM (WITH Other AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM OTHER) s; END",
    "Other"
)]
#[case::trigger_qualified_table(
    "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT picked FROM (WITH Other AS (SELECT 7 AS cte_only) SELECT id AS picked FROM main.other) s; END",
    "main.other"
)]
fn sqlite_cte_precedes_case_insensitive_catalog_lookup(
    #[case] sql: &str,
    #[case] expected_source: &str,
) -> testresult::TestResult {
    let oracle = test_helpers::sqlite_oracle()?;
    oracle.execute_batch("INSERT INTO items VALUES (42); INSERT INTO other VALUES (42);")?;
    let mut context = test_helpers::catalog(Dialect::sqlite());
    let statement = test_helpers::resolve(sql, &mut context)?;
    let command = if let Some(trigger) = statement.trigger() {
        oracle.execute_batch(sql)?;
        trigger.body().statements()[0].clone()
    } else {
        let actual: i64 = oracle.query_row(sql, [], |row| row.get(0))?;
        assert_eq!(
            actual,
            match expected_source {
                "Items" => 7,
                "items" => 9,
                _ => 42,
            }
        );
        statement
    };
    let mut query = command.into_select_statemtnt().unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert!(
        query
            .get_result_column_def(&crate::sem::ColumnName::from("picked"))
            .unwrap()
            .get_type_cloned()
            .is_some()
    );
    assert_eq!(test_helpers::bound_sources(&mut query)?, [expected_source]);
    Ok(())
}

#[rstest::rstest]
#[case::quoted_cte(
    r#"WITH "Items" AS (SELECT 7 AS cte_only) SELECT cte_only AS picked FROM "Items""#,
    "Items"
)]
#[case::case_sensitive_catalog_fallback(
    r#"WITH "Items" AS (SELECT 7 AS cte_only) SELECT id AS picked FROM ITEMS"#,
    "public.items"
)]
fn postgres_cte_lookup_preserves_quoted_case(
    #[case] sql: &str,
    #[case] expected_source: &str,
) -> testresult::TestResult {
    let mut context = test_helpers::catalog(Dialect::postgres());
    let mut query = test_helpers::resolve(sql, &mut context)?
        .into_select_statemtnt()
        .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    assert_eq!(test_helpers::bound_sources(&mut query)?, [expected_source]);
    Ok(())
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::{
        AnalysisError, BaseParseContext, ColumnDefinition, ColumnName, CreateTable,
        CreateTableMode, FrozenBaseContext, FullName, FunctionOverloadCollection, OverloadVariant,
        PgBuiltInType, SchemaFileContext, SchemaName, TableName, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    pub(super) fn bound_sources(
        query: &mut crate::sem::SelectStatement,
    ) -> Result<Vec<String>, AnalysisError> {
        #[derive(Default)]
        struct BoundSources(Vec<String>);
        impl crate::sem::traversal::SemanticVisitor for BoundSources {
            fn relation_identity(&mut self, name: &FullName) {
                self.0.push(name.to_string());
            }
            fn relation(&mut self, name: Option<&str>, _alias: &str, _columns: &[String]) {
                if let Some(name) = name {
                    self.0.push(name.to_owned());
                }
            }
        }
        let mut sources = BoundSources::default();
        query.walk(&mut sources)?;
        Ok(sources.0)
    }
    pub(super) fn catalog(dialect: Dialect) -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(dialect);
        let schema = if dialect == Dialect::sqlite() {
            "main"
        } else {
            "public"
        };
        context.set_get_search_path_result(&[SchemaName::from(schema)]);
        add_tables(&mut context, schema);
        let statement = crate::parse_statements("SELECT id FROM items", dialect)
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve(&mut context)
            .unwrap();
        let query = statement.into_select_statemtnt().unwrap();
        let view_name = FullName::with_schema(schema, "item_view");
        let view = crate::sem::CreateView::new(&view_name, &query, &[], &None, false);
        context.set_get_view_impl_result(&view_name, &Some(crate::sem::ViewLike::from(&view)));
        let overload = OverloadVariant::new(
            &Some(TypeReference::from_full_name(FullName::with_schema(
                "pg_catalog",
                "trigger",
            ))),
            &Vec::new(),
            false,
            false,
        );
        for (name, preserves) in [("audit_fn", true), ("set_trigger_fn", false)] {
            let name = FullName::with_schema("public", name);
            context.set_get_function_impl_result(
                &name,
                &FunctionOverloadCollection::new(std::slice::from_ref(&overload)),
            );
            context.set_function_row_property(&name, &overload, preserves);
        }
        let predicate =
            OverloadVariant::new(&Some(PgBuiltInType::bool()), &Vec::new(), false, false);
        for (name, preserves) in [("predicate_fn", true), ("aggregate_fn", false)] {
            let name = FullName::with_schema("public", name);
            context.set_get_function_impl_result(
                &name,
                &FunctionOverloadCollection::new(std::slice::from_ref(&predicate)),
            );
            context.set_function_row_property(&name, &predicate, preserves);
        }
        context
    }
    pub(super) fn add_tables(context: &mut TestParseContext, schema: &str) {
        for name in ["items", "other"] {
            let name = FullName::with_schema(schema, name);
            let column = ColumnDefinition::new(
                &Some(ColumnName::from("id")),
                Some(&PgBuiltInType::int4()),
                Some(false),
                &[],
            );
            let table = CreateTable::new(
                &TableName::from(name.clone()),
                vec![column],
                CreateTableMode::from_if_not_exists(false),
                false,
                &crate::test_helpers::source_span(),
            );
            context.set_get_table_impl_result(&name, &Some(table));
        }
    }
    pub(super) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("trigger.sql"),
            base,
            base.get_args(),
            base.metadata(),
        )
    }
    pub(super) fn resolve(
        sql: &str,
        context: &mut TestParseContext,
    ) -> Result<crate::sem::SemAst, AnalysisError> {
        crate::parse_statements(sql, crate::sem::ParseContext::dialect(context))
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve(context)
    }
    pub(super) fn apply(
        sql: &str,
        context: SchemaFileContext,
    ) -> Result<crate::AstAndContextPair<SchemaFileContext>, AnalysisError> {
        crate::parse_statements(sql, crate::sem::ParseContext::dialect(&context))
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve_and_apply(context)
    }
    #[cfg(feature = "sqlite")]
    pub(super) fn sqlite_oracle() -> Result<::rusqlite::Connection, ::rusqlite::Error> {
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch("CREATE TABLE items(id INTEGER); CREATE TABLE other(id INTEGER);")?;
        Ok(oracle)
    }
}
