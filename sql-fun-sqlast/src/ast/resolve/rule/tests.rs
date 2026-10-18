use crate::sem::{
    BaseParseContext, FullName, ParseContext, RuleEvent, RuleIdentity, TableInfoRead, ViewInfoRead,
};

#[rstest::rstest]
#[case::nothing(
    "CREATE RULE r AS ON INSERT TO items DO INSTEAD NOTHING",
    RuleEvent::insert(),
    0
)]
#[case::also_nothing(
    "CREATE RULE r AS ON UPDATE TO items DO NOTHING",
    RuleEvent::update(),
    0
)]
#[case::insert(
    "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink VALUES (NEW.id)",
    RuleEvent::insert(),
    1
)]
#[case::update(
    "CREATE RULE r AS ON UPDATE TO items DO UPDATE sink SET id=NEW.id WHERE id=OLD.id",
    RuleEvent::update(),
    1
)]
#[case::delete(
    "CREATE RULE r AS ON DELETE TO items DO DELETE FROM sink WHERE id=OLD.id",
    RuleEvent::delete(),
    1
)]
#[case::set_member_local_alias(
    "CREATE RULE r AS ON UPDATE TO items DO SELECT old.id FROM sink old UNION SELECT id FROM sink",
    RuleEvent::update(),
    1
)]
#[case::multiple(
    "CREATE RULE r AS ON UPDATE TO items DO (SELECT OLD.id; SELECT NEW.id)",
    RuleEvent::update(),
    2
)]
#[case::star(
    "CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.*",
    RuleEvent::update(),
    1
)]
#[case::correlated(
    "CREATE RULE r AS ON UPDATE TO items DO SELECT (SELECT OLD.id)",
    RuleEvent::update(),
    1
)]
#[case::cte(
    "CREATE RULE r AS ON UPDATE TO items DO WITH x AS (SELECT id FROM sink) SELECT id FROM x",
    RuleEvent::update(),
    1
)]
#[case::view(
    "CREATE RULE r AS ON UPDATE TO item_view DO SELECT NEW.id",
    RuleEvent::update(),
    1
)]
#[case::predicate(
    "CREATE RULE r AS ON UPDATE TO items WHERE OLD.id IS NOT NULL DO SELECT NEW.id",
    RuleEvent::update(),
    1
)]
#[case::null_predicate(
    "CREATE RULE r AS ON UPDATE TO items WHERE NULL DO NOTHING",
    RuleEvent::update(),
    0
)]
#[case::subquery_predicate(
    "CREATE RULE r AS ON UPDATE TO items WHERE (SELECT true FROM sink WHERE sink.id=OLD.id) DO NOTHING",
    RuleEvent::update(),
    0
)]
#[case::nested_shadow(
    "CREATE RULE r AS ON UPDATE TO items DO SELECT (SELECT old.id FROM sink old)",
    RuleEvent::update(),
    1
)]
#[case::returning(
    "CREATE RULE r AS ON INSERT TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id AS different",
    RuleEvent::insert(),
    1
)]
#[case::returning_old(
    "CREATE RULE r AS ON UPDATE TO items DO INSTEAD UPDATE sink SET id=NEW.id RETURNING OLD.id",
    RuleEvent::update(),
    1
)]
fn resolves_complete_statement_without_applying(
    #[case] sql: &str,
    #[case] event: RuleEvent,
    #[case] actions: usize,
) -> testresult::TestResult {
    let mut context = test_helpers::catalog();
    let statement = test_helpers::resolve(sql, &mut context)?;
    let rule = statement.rule().unwrap();
    assert_eq!(rule.event(), event);
    assert_eq!(rule.actions().len(), actions);
    assert!(context.rule(rule.identity()).is_none());
    assert!(rule.dependencies().relations().contains(rule.target()));
    assert!(rule.source().is_some());
    assert_eq!(context.reported_problem_count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::select_into("CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id INTO created")]
#[case::missing_target("CREATE RULE r AS ON INSERT TO absent DO NOTHING")]
#[case::bad_column("CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.absent")]
#[case::bad_old("CREATE RULE r AS ON INSERT TO items DO SELECT OLD.id")]
#[case::bad_new("CREATE RULE r AS ON DELETE TO items DO SELECT NEW.id")]
#[case::unqualified_action("CREATE RULE r AS ON UPDATE TO items DO SELECT id")]
#[case::ambiguous_predicate("CREATE RULE r AS ON UPDATE TO items WHERE id IS NOT NULL DO NOTHING")]
#[case::wrong_predicate_type("CREATE RULE r AS ON UPDATE TO items WHERE 1 DO NOTHING")]
#[case::bind_parameter("CREATE RULE r AS ON UPDATE TO items DO SELECT $1")]
#[case::where_bind_parameter("CREATE RULE r AS ON UPDATE TO items WHERE $1 DO NOTHING")]
#[case::reserved_name(r#"CREATE RULE "_RETURN" AS ON INSERT TO items DO NOTHING"#)]
#[case::select_table(
    r#"CREATE RULE "_RETURN" AS ON SELECT TO items DO INSTEAD SELECT id FROM sink"#
)]
#[case::select_name(
    "CREATE OR REPLACE RULE r AS ON SELECT TO item_view DO INSTEAD SELECT id FROM sink"
)]
#[case::select_nothing(
    r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD NOTHING"#
)]
#[case::select_also(
    r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO SELECT id FROM sink"#
)]
#[case::select_conditional(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view WHERE true DO INSTEAD SELECT id FROM sink"#)]
#[case::select_multiple(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD (SELECT id FROM sink; SELECT id FROM sink)"#)]
#[case::select_columns(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD SELECT id AS different FROM sink"#)]
#[case::select_modifying_cte(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD WITH x AS (DELETE FROM sink RETURNING id) SELECT id FROM x"#)]
#[case::select_old(
    r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD SELECT OLD.id"#
)]
#[case::returning_also(
    "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink VALUES(NEW.id) RETURNING id"
)]
#[case::returning_conditional(
    "CREATE RULE r AS ON INSERT TO items WHERE NEW.id IS NOT NULL DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id"
)]
#[case::returning_multiple(
    "CREATE RULE r AS ON INSERT TO items DO INSTEAD (INSERT INTO sink VALUES(NEW.id) RETURNING id; INSERT INTO sink VALUES(NEW.id) RETURNING id)"
)]
#[case::returning_width(
    "CREATE RULE r AS ON INSERT TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id,id"
)]
#[case::cte_rows(
    "CREATE RULE r AS ON UPDATE TO items DO WITH a AS (SELECT OLD.id) SELECT * FROM a"
)]
#[case::cte_namespace(
    "CREATE RULE r AS ON UPDATE TO items DO WITH old AS (SELECT id FROM sink) SELECT id FROM old"
)]
#[case::exists_pending(
    "CREATE RULE r AS ON UPDATE TO items WHERE EXISTS(SELECT id FROM sink) DO NOTHING"
)]
#[case::any_pending(
    "CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id = ANY(SELECT id FROM sink)"
)]
#[case::array_pending("CREATE RULE r AS ON UPDATE TO items DO SELECT ARRAY(SELECT id FROM sink)")]
#[case::source_namespace("CREATE RULE r AS ON UPDATE TO items DO SELECT old.id FROM sink old")]
#[case::insert_union_rows(
    "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink SELECT NEW.id UNION SELECT NEW.id"
)]
#[case::insert_source_namespace(
    "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink SELECT old.id FROM sink old"
)]
#[case::union_rows("CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id UNION SELECT NEW.id")]
fn rejects_invalid_definition_without_registration(#[case] sql: &str) {
    let mut context = test_helpers::catalog();
    assert!(test_helpers::resolve(sql, &mut context).is_err(), "{sql}");
    let identity = RuleIdentity::new(&FullName::with_schema("public", "items"), "r");
    assert!(context.rule(&identity).is_none());
    let mut has_rule_problem = false;
    for problem in context.reported_problems() {
        has_rule_problem |= problem.code() == "rule_invalid_definition";
    }
    assert!(has_rule_problem);
    assert!(
        context
            .get_table_impl(identity.target())
            .unwrap()
            .rules()
            .identities()
            .is_empty()
    );
}

#[test]
fn retains_bound_actions_and_dependencies() -> testresult::TestResult {
    let mut context = test_helpers::catalog();
    let statement = test_helpers::resolve(
        "CREATE RULE r AS ON UPDATE TO items DO (SELECT OLD.id; INSERT INTO sink VALUES(NEW.id))",
        &mut context,
    )?;
    let rule = statement.rule().unwrap();
    let query = rule.actions()[0].select_statement().unwrap();
    let reference = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .table_column_reference()
        .unwrap();
    assert_eq!(
        reference.relation(),
        &FullName::with_schema("public", "items")
    );
    assert_eq!(reference.alias().as_str(), "old");
    assert!(
        rule.dependencies()
            .relations()
            .contains(&FullName::with_schema("public", "sink"))
    );
    Ok(())
}

#[test]
fn named_lookup_target_links_and_layered_tombstones_agree() -> testresult::TestResult {
    let root = test_helpers::catalog().freeze();
    let mut context = test_helpers::layer(&root);
    for sql in [
        "CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.id",
        "CREATE RULE s AS ON INSERT TO items DO INSTEAD NOTHING",
        "CREATE RULE r AS ON UPDATE TO item_view DO SELECT OLD.id",
    ] {
        context = test_helpers::apply(sql, context)?;
    }
    let identity = RuleIdentity::new(&FullName::with_schema("public", "items"), "r");
    let view_identity = RuleIdentity::new(&FullName::with_schema("public", "item_view"), "r");
    assert!(context.rule(&identity).is_some());
    assert!(context.rule(&view_identity).is_some());
    assert_eq!(
        context
            .get_table_impl(identity.target())
            .unwrap()
            .rules()
            .resolve(&context)
            .len(),
        2
    );
    assert_eq!(
        context
            .get_view_impl(view_identity.target())
            .unwrap()
            .rules()
            .unwrap()
            .resolve(&context)
            .len(),
        1
    );
    let parent = context.freeze();
    let sibling = test_helpers::layer(&parent);
    let child = test_helpers::layer(&parent)
        .cancel_rule(&identity)?
        .dissociate_rule(identity.target(), &identity)?;
    assert!(child.rule(&identity).is_none());
    assert_eq!(
        child
            .get_table_impl(identity.target())
            .unwrap()
            .rules()
            .resolve(&child)
            .len(),
        1
    );
    assert!(parent.rule(&identity).is_some());
    assert_eq!(
        parent
            .get_table_impl(identity.target())
            .unwrap()
            .rules()
            .resolve(&parent)
            .len(),
        2
    );
    assert!(sibling.rule(&identity).is_some());
    assert!(child.rule(&view_identity).is_some());
    let descendant = test_helpers::layer(&child.freeze());
    assert!(descendant.rule(&identity).is_none());
    let recreated = test_helpers::apply(
        "CREATE RULE r AS ON UPDATE TO items DO INSTEAD NOTHING",
        descendant,
    )?;
    assert!(recreated.rule(&identity).unwrap().instead());
    assert_eq!(
        recreated
            .get_table_impl(identity.target())
            .unwrap()
            .rules()
            .resolve(&recreated)
            .len(),
        2
    );
    Ok(())
}

#[test]
fn replacement_updates_links_without_duplicates_and_rejected_analysis_preserves_parent()
-> testresult::TestResult {
    let root = test_helpers::catalog().freeze();
    let parent = test_helpers::apply(
        "CREATE RULE r AS ON INSERT TO item_view DO INSTEAD NOTHING",
        test_helpers::layer(&root),
    )?
    .freeze();
    let identity = RuleIdentity::new(&FullName::with_schema("public", "item_view"), "r");
    let mut child = test_helpers::layer(&parent);
    assert!(
        test_helpers::resolve(
            "CREATE OR REPLACE RULE r AS ON INSERT TO item_view DO SELECT OLD.id",
            &mut child
        )
        .is_err()
    );
    assert!(child.rule(&identity).unwrap().instead());
    assert!(
        test_helpers::apply(
            "CREATE RULE r AS ON INSERT TO item_view DO NOTHING",
            test_helpers::layer(&parent)
        )
        .is_err()
    );
    let child = test_helpers::apply(
        "CREATE OR REPLACE RULE r AS ON INSERT TO item_view DO SELECT NEW.id",
        test_helpers::layer(&parent),
    )?;
    let view = child.get_view_impl(identity.target()).unwrap();
    let active = view.rules().unwrap().resolve(&child);
    assert_eq!(active.len(), 1);
    assert!(!active[0].instead());
    assert!(std::ptr::eq(active[0], child.rule(&identity).unwrap()));
    assert!(parent.rule(&identity).unwrap().instead());
    Ok(())
}

#[test]
fn select_rule_replacement_updates_the_target_query() -> testresult::TestResult {
    let root = test_helpers::catalog().freeze();
    let context = test_helpers::apply(
        r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD SELECT id FROM sink"#,
        test_helpers::layer(&root),
    )?;
    let identity = RuleIdentity::new(&FullName::with_schema("public", "item_view"), "_RETURN");
    assert_eq!(
        context
            .get_view_impl(identity.target())
            .unwrap()
            .rules()
            .unwrap()
            .resolve(&context)
            .len(),
        1
    );
    assert!(
        context
            .rule(&identity)
            .unwrap()
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("public", "sink")),
        "{:?}",
        context.rule(&identity).unwrap()
    );
    Ok(())
}

#[test]
fn exact_identities_preserve_relation_scope_and_quoted_components() -> testresult::TestResult {
    let mut catalog = test_helpers::catalog();
    for name in [
        FullName::with_schema("public", "a.b"),
        FullName::with_schema("public.a", "b"),
    ] {
        test_helpers::add_table(&mut catalog, &name);
    }
    let root = catalog.freeze();
    let mut context = test_helpers::layer(&root);
    for sql in [
        "CREATE RULE r AS ON INSERT TO items DO NOTHING",
        "CREATE RULE r AS ON INSERT TO sink DO NOTHING",
        r#"CREATE RULE "R.x" AS ON INSERT TO public."a.b" DO NOTHING"#,
        r#"CREATE RULE "R.x" AS ON INSERT TO "public.a".b DO NOTHING"#,
    ] {
        context = test_helpers::apply(sql, context)?;
    }
    for (target, name) in [
        (FullName::with_schema("public", "items"), "r"),
        (FullName::with_schema("public", "sink"), "r"),
        (FullName::with_schema("public", "a.b"), "R.x"),
        (FullName::with_schema("public.a", "b"), "R.x"),
    ] {
        let identity = RuleIdentity::new(&target, name);
        let registered = context.rule(&identity).unwrap();
        assert_eq!(registered.target(), &target);
        let linked = context
            .get_table_impl(&target)
            .unwrap()
            .rules()
            .resolve(&context);
        assert_eq!(linked.len(), 1);
        assert!(std::ptr::eq(linked[0], registered));
    }
    let error = test_helpers::resolve(
        "CREATE RULE r AS ON INSERT TO items DO NOTHING",
        &mut context,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Duplicate"));
    Ok(())
}

#[rstest::rstest]
#[case("15", false)]
#[case("16", false)]
#[case("17", false)]
#[case("18", true)]
fn insert_returning_uses_versioned_rule_scope(
    #[case] version: &str,
    #[case] accepted: bool,
) -> testresult::TestResult {
    let metadata = ::serde_json::from_value(::serde_json::json!({
        "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
    }))?;
    let mut context = test_helpers::catalog().with_metadata(&metadata);
    let result = test_helpers::resolve(
        "CREATE RULE r AS ON UPDATE TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING OLD.id",
        &mut context,
    );
    assert_eq!(result.is_ok(), accepted, "{version}: {result:?}");
    Ok(())
}

#[test]
fn modifier_capable_results_have_an_explicit_semantic_boundary() -> testresult::TestResult {
    let mut context = test_helpers::catalog();
    let name = FullName::with_schema("public", "sized");
    let column = crate::sem::ColumnDefinition::new(
        &Some(crate::sem::ColumnName::from("id")),
        Some(&crate::sem::TypeReference::concrete_type_ref(
            &FullName::with_schema("pg_catalog", "varchar"),
            false,
        )),
        Some(false),
        &[],
    );
    let table = crate::sem::CreateTable::new(
        &crate::sem::TableName::from(name.clone()),
        vec![column],
        crate::sem::CreateTableMode::from_if_not_exists(false),
        false,
        &crate::test_helpers::source_span(),
    );
    context.set_get_table_impl_result(&name, &Some(table));
    let query = test_helpers::resolve("SELECT id FROM sized", &mut context)?
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("public", "sized_view");
    let view = crate::sem::CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(crate::sem::ViewLike::from(&view)));
    let error = test_helpers::resolve(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO sized_view DO INSTEAD SELECT id FROM sized"#, &mut context).unwrap_err();
    assert!(error.to_string().contains("#619"));
    assert!(context.rule(&RuleIdentity::new(&name, "_RETURN")).is_none());
    Ok(())
}

#[test]
fn action_lookup_retains_the_first_relation_on_the_search_path() -> testresult::TestResult {
    let mut context = test_helpers::catalog();
    let query = test_helpers::resolve("SELECT id FROM public.sink", &mut context)?
        .into_select_statemtnt()
        .unwrap();
    let name = FullName::with_schema("pg_catalog", "items");
    let view = crate::sem::CreateView::new(&name, &query, &[], &None, false);
    context.set_get_view_impl_result(&name, &Some(crate::sem::ViewLike::from(&view)));
    let statement = test_helpers::resolve(
        "CREATE RULE r AS ON UPDATE TO public.sink DO SELECT id FROM items",
        &mut context,
    )?;
    let rule = statement.rule().unwrap();
    assert!(rule.dependencies().relations().contains(&name));
    assert!(
        !rule
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("public", "items"))
    );
    Ok(())
}

#[test]
fn retains_nested_cte_write_dependencies() -> testresult::TestResult {
    let mut context = test_helpers::catalog();
    let statement = test_helpers::resolve(
        "CREATE RULE r AS ON UPDATE TO items DO WITH x AS (INSERT INTO sink VALUES(1) RETURNING id) SELECT id FROM x",
        &mut context,
    )?;
    assert!(
        statement
            .rule()
            .unwrap()
            .dependencies()
            .relations()
            .contains(&FullName::with_schema("public", "sink"))
    );
    Ok(())
}

mod test_helpers {
    use crate::ast::Dialect;
    use crate::sem::{
        AnalysisError, BaseParseContext, ColumnDefinition, ColumnName, CreateTable,
        CreateTableMode, FrozenBaseContext, FullName, PgBuiltInType, SchemaFileContext, SchemaName,
        SemAst, TableName,
    };
    use crate::test_helpers::TestParseContext;

    pub(super) fn catalog() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(Dialect::postgres());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        crate::sem::MutSourcePathWithSpan::set_source_path_with_span(
            &mut context,
            &crate::test_helpers::source_span(),
        );
        let int = PgBuiltInType::int4();
        context.set_get_implicit_cast_result(
            &int,
            &int,
            Some(crate::sem::CastDefinition::new(
                crate::sem::CastContext::NoConversion,
            )),
        );
        let equality =
            crate::sem::OperatorDefinition::binary(&[crate::sem::BinaryOperatorDefinition::new(
                &int,
                &int,
                &PgBuiltInType::bool(),
                true,
            )]);
        context.set_get_operator_op_result(
            &crate::sem::OperatorName::from_parts(None, "="),
            true,
            true,
            Some(&equality),
        );
        for name in ["items", "sink"] {
            add_table(&mut context, &FullName::with_schema("public", name));
        }
        let query = resolve("SELECT id FROM items", &mut context)
            .unwrap()
            .into_select_statemtnt()
            .unwrap();
        let name = FullName::with_schema("public", "item_view");
        let view = crate::sem::CreateView::new(&name, &query, &[], &None, false);
        context.set_get_view_impl_result(&name, &Some(crate::sem::ViewLike::from(&view)));
        context
    }
    pub(super) fn add_table(context: &mut TestParseContext, name: &FullName) {
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
        context.set_get_table_impl_result(name, &Some(table));
    }
    pub(super) fn resolve(
        sql: &str,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<SemAst, AnalysisError> {
        crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve(context)
    }
    pub(super) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("rule.sql"),
            base,
            base.get_args(),
            base.metadata(),
        )
    }
    pub(super) fn apply(
        sql: &str,
        context: SchemaFileContext,
    ) -> Result<SchemaFileContext, AnalysisError> {
        Ok(crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap()
            .node
            .resolve_and_apply(context)?
            .into_context())
    }
}
