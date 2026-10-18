use crate::sem::{FullName, ParseContext, RuleIdentity, RuleInfoRead};

#[rstest::rstest]
#[case::restrict(
    "DROP RULE audit ON item_view RESTRICT",
    crate::sem::DropDependencyBehavior::restrict()
)]
#[case::cascade(
    "DROP RULE audit ON public.item_view CASCADE",
    crate::sem::DropDependencyBehavior::cascade()
)]
fn resolved_drop_does_not_apply_until_requested(
    #[case] sql: &str,
    #[case] behavior: crate::sem::DropDependencyBehavior,
) -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let mut child = test_helpers::layer(&parent);
    let statement = test_helpers::statement(sql).node.resolve(&mut child)?;
    let drop = statement.rule_drop().unwrap();
    assert!(drop.found());
    assert_eq!(drop.identity(), Some(&test_helpers::identity()));
    assert_eq!(drop.behavior(), behavior);
    assert!(!drop.if_exists());
    assert!(child.rule(&test_helpers::identity()).is_some());
    Ok(())
}

#[test]
fn drop_masks_inherited_definition_and_recreation_replaces_the_tombstone() -> testresult::TestResult
{
    let parent = test_helpers::catalog()?;
    let child = test_helpers::apply(test_helpers::layer(&parent), "DROP RULE audit ON item_view")?
        .into_context()
        .freeze();
    assert!(child.rule(&test_helpers::identity()).is_none());
    assert!(
        test_helpers::layer(&child)
            .rule(&test_helpers::identity())
            .is_none()
    );
    assert!(parent.rule(&test_helpers::identity()).is_some());
    assert!(
        test_helpers::layer(&parent)
            .rule(&test_helpers::identity())
            .is_some()
    );
    assert_eq!(test_helpers::active_count(&parent), 2);
    assert_eq!(test_helpers::active_count(&child), 1);
    let grandchild = test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
        .into_context()
        .freeze();
    assert!(grandchild.rule(&test_helpers::identity()).is_some());
    assert_eq!(test_helpers::active_count(&grandchild), 2);
    assert!(child.rule(&test_helpers::identity()).is_none());
    Ok(())
}

#[rstest::rstest]
#[case::missing_rule("DROP RULE IF EXISTS missing ON item_view", true)]
#[case::missing_relation("DROP RULE IF EXISTS audit ON absent", false)]
fn if_exists_retains_completed_absence_without_changing_associations(
    #[case] sql: &str,
    #[case] relation_found: bool,
) -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let pair = test_helpers::apply(test_helpers::layer(&parent), sql)?;
    let drop = pair.ast().rule_drop().unwrap();
    assert!(!drop.found());
    assert!(drop.if_exists());
    assert_eq!(drop.identity().is_some(), relation_found);
    assert_eq!(test_helpers::active_count(&pair.into_context().freeze()), 2);
    Ok(())
}

#[rstest::rstest]
#[case::missing_rule("DROP RULE missing ON item_view")]
#[case::missing_relation("DROP RULE audit ON absent")]
fn invalid_drop_leaves_named_definitions_and_links_intact(
    #[case] sql: &str,
) -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let mut child = test_helpers::layer(&parent);
    assert!(
        test_helpers::statement(sql)
            .node
            .resolve(&mut child)
            .is_err()
    );
    assert!(child.rule(&test_helpers::identity()).is_some());
    assert_eq!(test_helpers::active_count(&child.freeze()), 2);
    Ok(())
}

#[test]
fn relation_namespace_and_quoted_rule_names_remain_distinct() -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let context = test_helpers::apply(
        test_helpers::layer(&parent),
        r#"CREATE RULE "Audit.Name" AS ON INSERT TO items DO ALSO NOTHING"#,
    )?
    .into_context();
    let context = test_helpers::apply(
        context,
        "CREATE RULE audit AS ON INSERT TO items DO ALSO NOTHING",
    )?
    .into_context();
    let pair = test_helpers::apply(context, r#"DROP RULE "Audit.Name" ON public.items CASCADE"#)?;
    let quoted = RuleIdentity::new(&FullName::with_schema("public", "items"), "Audit.Name");
    let context = pair.into_context().freeze();
    assert!(context.rule(&quoted).is_none());
    assert!(context.rule(&test_helpers::identity()).is_some());
    let table_identity = RuleIdentity::new(&FullName::with_schema("public", "items"), "audit");
    assert!(context.rule(&table_identity).is_some());
    assert_eq!(test_helpers::table_rule_count(&context), 1);
    Ok(())
}

#[test]
fn relation_search_path_does_not_fall_through_to_a_later_rule() -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let mut context = test_helpers::apply(
        test_helpers::layer(&parent),
        "CREATE TABLE earlier.item_view(id integer)",
    )?
    .into_context();
    context.set_search_path(&[
        crate::sem::SchemaName::from("earlier"),
        crate::sem::SchemaName::from("public"),
    ]);
    let pair = test_helpers::apply(context, "DROP RULE IF EXISTS audit ON item_view")?;
    let drop = pair.ast().rule_drop().unwrap();
    assert!(!drop.found());
    assert_eq!(
        drop.identity().unwrap().target(),
        &FullName::with_schema("earlier", "item_view")
    );
    assert!(
        pair.into_context()
            .rule(&test_helpers::identity())
            .is_some()
    );
    Ok(())
}

mod test_helpers {
    use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
    use crate::sem::{BaseParseContext, FullName, ParseContext, RuleIdentity, TableName, ViewName};
    pub(super) const CREATE_AUDIT: &str =
        "CREATE RULE audit AS ON INSERT TO item_view DO INSTEAD NOTHING";
    pub(super) fn identity() -> RuleIdentity {
        RuleIdentity::new(&FullName::with_schema("public", "item_view"), "audit")
    }
    // Small explicitly selected builtin fixture; no full production master is loaded.
    pub(super) fn catalog() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
        use ::clap::Parser;
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let args = ::sql_fun_core::SqlFunArgs::try_parse_from([
            "sql-fun",
            "--metadata-file",
            manifest
                .join("tests/fixtures/writable-postgres.toml")
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
            std::path::Path::new("drop-rule.sql"),
            &base,
            &args,
            &metadata,
        );
        for sql in [
            "CREATE TABLE public.items(id integer)",
            "CREATE VIEW public.item_view AS SELECT id FROM public.items",
            CREATE_AUDIT,
            "CREATE RULE other AS ON UPDATE TO item_view DO INSTEAD NOTHING",
        ] {
            context = apply(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(context.freeze())
    }
    pub(super) fn layer(base: &FrozenBaseContext) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("drop-rule.sql"),
            base,
            base.get_args(),
            base.metadata(),
        )
    }
    pub(super) fn statement(sql: &str) -> crate::ast::Statement {
        crate::parse_statements(sql, crate::ast::Dialect::postgres())
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
    pub(super) fn table_rule_count(context: &FrozenBaseContext) -> usize {
        let target = FullName::with_schema("public", "items");
        context
            .get_table(&TableName::from(target.clone()))
            .unwrap()
            .rules()
            .resolve(context)
            .len()
    }
    pub(super) fn active_count(context: &FrozenBaseContext) -> usize {
        let target = FullName::with_schema("public", "item_view");
        context
            .get_view(&ViewName::from_full_name(&target))
            .unwrap()
            .rules()
            .unwrap()
            .resolve(context)
            .len()
    }
}

#[rstest::rstest]
#[case::implicit_restrict("RESTRICT", false)]
#[case::implicit_cascade("CASCADE", false)]
#[case::explicit_restrict("RESTRICT", true)]
#[case::explicit_cascade("CASCADE", true)]
fn cannot_drop_the_select_rule_required_by_a_view(
    #[case] behavior: &str,
    #[case] explicit: bool,
) -> testresult::TestResult {
    let parent = test_helpers::catalog()?;
    let context = if explicit {
        test_helpers::apply(test_helpers::layer(&parent), "CREATE OR REPLACE RULE \"_RETURN\" AS ON SELECT TO public.item_view DO INSTEAD SELECT id FROM public.items")?.into_context().freeze()
    } else {
        parent
    };
    let mut child = test_helpers::layer(&context);
    let result = test_helpers::statement(&format!(
        "DROP RULE \"_RETURN\" ON public.item_view {behavior}"
    ))
    .node
    .resolve(&mut child);
    let Err(crate::sem::AnalysisError::Rule(problem)) = result else {
        panic!("expected rule diagnostic")
    };
    assert!(
        problem
            .definition_detail()
            .unwrap()
            .contains("View requires")
    );
    let identity = RuleIdentity::new(&FullName::with_schema("public", "item_view"), "_RETURN");
    assert_eq!(child.rule(&identity).is_some(), explicit);
    assert_eq!(
        test_helpers::active_count(&child.freeze()),
        if explicit { 3 } else { 2 }
    );
    Ok(())
}
