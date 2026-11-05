use crate::{
    AstAndContextPair,
    ast::{Dialect, DropTrigger},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, DropDependencyBehavior, DropTrigger as SemDropTrigger, ResolutionFailure,
        SemAst, TriggerIdentity, problem::TriggerProblem,
    },
};

impl SemDropTrigger {
    pub(crate) fn from_ast(
        ast: &DropTrigger,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = if context.dialect() == Dialect::sqlite() {
            Self::from_ast_sqlite(ast, context)
        } else {
            Self::from_ast_postgres(ast, context)
        };
        ResolutionFailure::finish(result, context)
    }
}

#[cfg(test)]
mod test_from_ast {
    use crate::{
        context::{ParseContext, TriggerInfoRead},
        names::{FullName, SchemaName},
        sem::{SemAst, TriggerIdentity},
        test_helpers::{
            create_cache_context as test_helpers_create_cache_context,
            drop_trigger as test_helpers, heavy::drop_trigger as heavy,
        },
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
            test_helpers::statement(sql).node(),
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
    fn drop_masks_inherited_definition_and_recreation_replaces_the_tombstone()
    -> testresult::TestResult {
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
        let grandchild =
            test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
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
                test_helpers::statement(sql).node(),
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

    #[cfg(feature = "sqlite")]
    mod sqlite {
        use crate::{
            context::{ParseContext, TriggerInfoRead},
            names::FullName,
            sem::{SemAst, TriggerIdentity},
            test_helpers::{
                drop_trigger_sqlite as test_helpers, heavy::drop_trigger_sqlite as heavy,
            },
        };

        #[test]
        fn drop_masks_inherited_trigger_and_recreation_preserves_other_links()
        -> testresult::TestResult {
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
            let recreated =
                test_helpers::apply(test_helpers::layer(&child), test_helpers::CREATE_AUDIT)?
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
                SemAst::from_ast_node(
                    test_helpers::statement("DROP TRIGGER absent").node(),
                    &mut context,
                    &mut crate::test_helpers::create_cache_context()
                )
                .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement))
            );
            assert_eq!(test_helpers::active_count(&context.freeze()), 2);
            Ok(())
        }

        #[test]
        fn temp_trigger_precedes_main_and_qualified_drop_selects_its_namespace()
        -> testresult::TestResult {
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
            let selected_identity =
                TriggerIdentity::sqlite(&FullName::with_schema(selected, "audit"));
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
    }
}

impl SemDropTrigger {
    pub(crate) fn from_ast_sqlite(
        ast: &DropTrigger,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(ast.name())?;
        let identity = if requested.schema().is_some() {
            let identity = TriggerIdentity::sqlite(&requested);
            if let Some(definition) = context.trigger(&identity) {
                return Ok(Self::sqlite(
                    &requested,
                    Some(&identity),
                    Some(definition.target()),
                    ast.if_exists(),
                ));
            }
            Some(identity)
        } else {
            let mut schemas = vec![
                crate::names::SchemaName::from("temp"),
                crate::names::SchemaName::from("main"),
            ];
            for schema in context.get_search_path() {
                if !schemas
                    .iter()
                    .any(|existing| existing.as_str().eq_ignore_ascii_case(schema.as_str()))
                {
                    schemas.push(schema.clone());
                }
            }
            for schema in schemas {
                let identity = TriggerIdentity::sqlite(&requested.clone().set_schema(schema));
                if let Some(definition) = context.trigger(&identity) {
                    return Ok(Self::sqlite(
                        &requested,
                        Some(&identity),
                        Some(definition.target()),
                        ast.if_exists(),
                    ));
                }
            }
            None
        };
        if !ast.if_exists() {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing_name(
                &requested,
            )));
        }
        Ok(Self::sqlite(&requested, identity.as_ref(), None, true))
    }
}

impl SemDropTrigger {
    pub(crate) fn from_ast_postgres(
        ast: &DropTrigger,
        context: &impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let requested = FullName::from_ast_name(ast.name())?;
        let requested_target =
            FullName::from_ast_name(ast.relation().as_ref().expect("PostgreSQL ON relation"))?;
        let behavior = DropDependencyBehavior::from_ast(
            &ast.behavior().expect("PostgreSQL dependency behavior"),
        );
        let Some(target) = context.resolve_relation_name(&requested_target) else {
            if ast.if_exists() {
                return Ok(Self::new(
                    &requested,
                    &requested_target,
                    None,
                    false,
                    true,
                    behavior,
                ));
            }
            return Err(ResolutionFailure::trigger(&TriggerProblem::target(
                &requested_target,
            )));
        };
        Self::from_ast_on(
            ast,
            context,
            &requested,
            &requested_target,
            &target,
            behavior,
        )
    }
}

impl SemDropTrigger {
    pub(crate) fn from_ast_on(
        ast: &DropTrigger,
        context: &impl ParseContext,
        requested: &FullName,
        requested_target: &FullName,
        target: &FullName,
        behavior: DropDependencyBehavior,
    ) -> Result<Self, ResolutionFailure> {
        let name = FullName::new(
            &target.catalog().cloned(),
            &target.schema().cloned(),
            requested.name(),
        );
        let identity = TriggerIdentity::postgres(target, &name);
        let found = context.trigger(&identity).is_some();
        if !found && !ast.if_exists() {
            return Err(ResolutionFailure::trigger(&TriggerProblem::missing(
                &identity,
            )));
        }
        Ok(Self::new(
            requested,
            requested_target,
            Some(&identity),
            found,
            ast.if_exists(),
            behavior,
        ))
    }
}

impl SemDropTrigger {
    pub(crate) fn apply_ast<C: ParseContext>(
        ast: &DropTrigger,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let Some(definition) = Self::from_ast(ast, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = definition.apply(context)?;
        Ok(AstAndContextPair::new(
            SemAst::drop_trigger(&definition),
            context,
        ))
    }
}
