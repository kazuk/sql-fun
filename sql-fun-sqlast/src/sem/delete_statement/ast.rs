use crate::{
    CacheContext,
    ast::{Delete, Dialect},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CreateTrigger as SemCreateTrigger, CteStatement,
        DataSource, DeleteCursor, DeleteDestination, DeletePredicate, DeleteStatement, FromClause,
        PgBuiltInType, ResolutionFailure, ResultColumnCollection, ScalarAnalysisContext, SemAst,
        SemScalarExpr, WithClause, parameter_resolution::ParameterResolution,
        problem::DeleteProblemKind,
    },
};

impl DeleteStatement {
    pub(crate) fn from_ast(
        ast: &Delete,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|mut delete| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_delete(&mut delete, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(delete)
        });
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_from_ast {
    #[cfg(feature = "sqlite")]
    use crate::{
        parse_statements,
        sem::{SemAst, SemAst as SemSemAst},
        test_helpers::SqliteSynBuilder,
    };
    mod cursors {
        use crate::{
            ast::{Dialect, Expr, StatementNode},
            context::ParseContext,
            names::{FullName, TableName},
            parse_statements,
            sem::{CursorInfo, SemAst},
            test_helpers::{
                create_cache_context, delete::catalog, statements::resolve_cursor as resolve,
            },
        };

        #[rstest::rstest]
        #[case::target(Some("items"), Some(true), false)]
        #[case::unknown_target(None, Some(true), false)]
        #[case::unknown_eligibility(Some("items"), None, false)]
        #[case::unknown_facts(None, None, false)]
        #[case::wrong_target(Some("other"), Some(true), true)]
        #[case::read_only(Some("items"), Some(false), true)]
        fn resolves_portal_in_current_context(
            #[case] target: Option<&str>,
            #[case] updatable: Option<bool>,
            #[case] invalid: bool,
        ) {
            let mut context = catalog(Dialect::postgres());
            let target = target.map(|name| TableName::from(FullName::with_schema("public", name)));
            let info = CursorInfo::new(target.as_ref(), updatable);
            context.bind_cursor("portal", &info);
            let statement = resolve(
                "DELETE FROM items AS i WHERE CURRENT OF portal RETURNING $1::integer",
                &mut context,
            );
            let delete = statement.as_delete().unwrap();
            assert!(delete.predicate().is_none());
            let cursor = delete.current_of().unwrap();
            assert_eq!(cursor.name(), "portal");
            assert_eq!(cursor.binding(), Some(&info));
            assert_eq!(
                context.reported_problem_count() > 0,
                invalid,
                "{:?}",
                context.reported_problems()
            );
            if updatable == Some(false) {
                assert_eq!(
                    context.reported_problems()[0]
                        .delete_problem()
                        .unwrap()
                        .kind(),
                    &crate::sem::problem::DeleteProblemKind::non_updatable_cursor("portal")
                );
            } else if invalid {
                assert_eq!(
                    context.reported_problems()[0]
                        .delete_problem()
                        .unwrap()
                        .kind(),
                    &crate::sem::problem::DeleteProblemKind::cursor_target_mismatch(
                        "portal",
                        delete.target(),
                        target.as_ref().unwrap()
                    )
                );
            }
            assert!(
                context
                    .parameter_constraints()
                    .parameter(1)
                    .unwrap()
                    .constraint()
                    .allows(&crate::sem::PgBuiltInType::int4())
            );
        }

        #[rstest::rstest]
        #[case::empty_scope(false)]
        #[case::different_portal(true)]
        fn retains_missing_reference(#[case] other_binding: bool) {
            let mut context = catalog(Dialect::postgres());
            if other_binding {
                context.bind_cursor("other", &CursorInfo::new(None, None));
            }
            let statement = resolve("DELETE FROM items WHERE CURRENT OF absent", &mut context);
            let cursor = statement.as_delete().unwrap().current_of().unwrap();
            assert_eq!(cursor.name(), "absent");
            assert!(cursor.binding().is_none());
            assert_eq!(context.reported_problem_count(), 1);
            assert_eq!(
                context.reported_problems()[0]
                    .delete_problem()
                    .unwrap()
                    .kind(),
                &crate::sem::problem::DeleteProblemKind::missing_cursor("absent")
            );
        }

        #[test]
        fn portal_scope_survives_statement_reset_and_can_be_replaced() {
            let mut context = catalog(Dialect::postgres());
            context.bind_cursor("portal", &CursorInfo::new(None, None));
            let first = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
            let second = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
            assert!(
                first
                    .as_delete()
                    .unwrap()
                    .current_of()
                    .unwrap()
                    .binding()
                    .is_some()
            );
            assert!(
                second
                    .as_delete()
                    .unwrap()
                    .current_of()
                    .unwrap()
                    .binding()
                    .is_some()
            );
            let replacement = CursorInfo::new(None, Some(false));
            context.bind_cursor("portal", &replacement);
            let third = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
            assert_eq!(
                third.as_delete().unwrap().current_of().unwrap().binding(),
                Some(&replacement)
            );
            // Semantic results are snapshots, independent of subsequent scope changes.
            assert_eq!(
                first
                    .as_delete()
                    .unwrap()
                    .current_of()
                    .unwrap()
                    .binding()
                    .unwrap()
                    .updatable(),
                None
            );
            context.remove_cursor("portal");
            let last = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
            assert!(
                last.as_delete()
                    .unwrap()
                    .current_of()
                    .unwrap()
                    .binding()
                    .is_none()
            );
        }

        #[rstest::rstest]
        #[case::quoted(r#"DELETE FROM items WHERE CURRENT OF "Mixed""#, "Mixed", false)]
        #[case::unquoted("DELETE FROM items WHERE CURRENT OF Mixed", "mixed", true)]
        fn portal_names_follow_identifier_rules(
            #[case] sql: &str,
            #[case] name: &str,
            #[case] missing: bool,
        ) {
            let mut context = catalog(Dialect::postgres());
            context.bind_cursor("Mixed", &CursorInfo::new(None, None));
            let statement = resolve(sql, &mut context);
            let cursor = statement.as_delete().unwrap().current_of().unwrap();
            assert_eq!(cursor.name(), name);
            assert_eq!(cursor.binding().is_none(), missing);
        }

        #[test]
        fn retains_unsupported_procedural_cursor_parameter() {
            let mut context = catalog(Dialect::postgres());
            let mut statement = parse_statements(
                "DELETE FROM items WHERE CURRENT OF portal",
                Dialect::postgres(),
            )
            .next()
            .unwrap()
            .unwrap();
            let StatementNode::Delete(delete) = &mut statement.node else {
                panic!("DELETE")
            };
            let Some(Expr::CurrentOf(cursor)) = &mut delete.predicate else {
                panic!("CURRENT OF")
            };
            cursor.parameter = 3;
            let result =
                SemAst::from_ast_node(&statement.node, &mut context, &mut create_cache_context())
                    .unwrap();
            assert_eq!(
                result
                    .as_delete()
                    .unwrap()
                    .current_of()
                    .unwrap()
                    .parameter_number(),
                Some(3)
            );
            assert_eq!(
                context.reported_problems()[0]
                    .delete_problem()
                    .unwrap()
                    .kind(),
                &crate::sem::problem::DeleteProblemKind::cursor_parameter(3)
            );
        }
    }
    mod sql_driven {
        // DELETE analysis through the public catalog and statement APIs.

        use crate::{
            ast::Dialect,
            context::{BaseParseContext, ParseContext},
            names::{AliasName, ColumnName, FullName, TableName},
            parse_statements,
            sem::{CteStatement, DataSource, PgBuiltInType, SelectStatement, SemAst},
            test_helpers::{create_cache_context, delete_sql_driven as test_helpers},
        };

        #[test]
        fn sql_driven_delete_cte_preserves_bindings_types_and_schema()
        -> Result<(), Box<dyn std::error::Error>> {
            let mut cache_context = create_cache_context();
            let mut context = test_helpers::context();
            for statement in parse_statements(
                "CREATE TABLE items(id integer NOT NULL, label text); CREATE TABLE sources(id integer, label text)",
                Dialect::postgres(),
            ) {
                context =
                    SemAst::from_ast(&statement?, context, &mut cache_context)?.into_context();
            }
            let statement = parse_statements("WITH d(removed,label) AS (DELETE FROM items AS i USING sources AS s WHERE i.id=s.id AND i.id=$1 RETURNING i.id,s.label) SELECT removed,label FROM d", Dialect::postgres()).next().unwrap()?;
            let analyzed = SemAst::from_ast(&statement, context, &mut cache_context)?;
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let constraints = analyzed.context().parameter_constraints();
            assert!(
                constraints
                    .parameter(1)
                    .unwrap()
                    .constraint()
                    .allows(&PgBuiltInType::int4())
            );
            assert!(
                !constraints
                    .parameter(1)
                    .unwrap()
                    .constraint()
                    .allows(&PgBuiltInType::bool())
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let SelectStatement::Query(boundary) = &query else {
                panic!("query boundary")
            };
            let CteStatement::Delete(delete) = boundary.with()[0].statement() else {
                panic!("DELETE CTE")
            };
            assert_eq!(delete.target().to_string(), "public.items");
            for (alias, relation) in [("i", "items"), ("s", "sources")] {
                let source = delete
                    .from()
                    .resolve_alias(&AliasName::from(alias))
                    .unwrap()
                    .0;
                let DataSource::Table(table) = source else {
                    panic!("catalog table")
                };
                let expected = TableName::from(FullName::with_schema("public", relation));
                assert_eq!(
                    analyzed.context().get_table(table.name()).unwrap().name(),
                    analyzed.context().get_table(&expected).unwrap().name()
                );
            }
            assert!(delete.predicate().is_some());
            let source_column = delete.result_columns().get_column_at(1).unwrap();
            assert_eq!(source_column.col_type(), &Some(PgBuiltInType::text()));
            assert_eq!(source_column.is_not_null(), &Some(false));
            assert!(
                source_column
                    .expression()
                    .unwrap()
                    .get_column_def()
                    .is_some()
            );
            let column = query.result_columns().get_column_at(0).unwrap();
            assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            assert_eq!(column.is_not_null(), &Some(true));
            assert!(
                analyzed
                    .context()
                    .get_table(&TableName::from(FullName::with_schema("public", "items")))
                    .is_some()
            );
            Ok(())
        }
    }
    mod versions {
        use crate::{
            ast::Dialect,
            test_helpers::{delete::resolve, delete_versions as test_helpers},
        };

        #[rstest::rstest]
        #[case::postgres17("17", true)]
        #[case::postgres18("18", false)]
        #[case::custom("custom-engine", true)]
        fn returning_row_images_use_configured_engine_version(
            #[case] version: &str,
            #[case] missing: bool,
        ) {
            let mut context = test_helpers::catalog_for_version(version);
            let statement = resolve(
                "DELETE FROM items RETURNING old.id,new.id",
                &mut context,
                Dialect::postgres(),
            )
            .unwrap();
            assert_eq!(context.reported_problem_count() > 0, missing);
            if !missing {
                let columns = statement.as_delete().unwrap().result_columns();
                assert_eq!(columns.get_column_at(0).unwrap().is_not_null(), &Some(true));
                assert_eq!(
                    columns.get_column_at(1).unwrap().is_not_null(),
                    &Some(false)
                );
            }
        }

        #[rstest::rstest]
        #[case::old_star("DELETE FROM items RETURNING old.*", 2)]
        #[case::new_star("DELETE FROM items RETURNING new.*", 2)]
        #[case::nested("DELETE FROM items RETURNING (SELECT old.id)", 1)]
        fn row_images_preserve_typed_catalog_bindings(#[case] sql: &str, #[case] width: usize) {
            let mut context = test_helpers::catalog_for_version("18");
            let result = resolve(sql, &mut context, Dialect::postgres()).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(result.as_delete().unwrap().result_columns().len(), width);
            if sql.contains(".*") {
                let column = result
                    .as_delete()
                    .unwrap()
                    .result_columns()
                    .get_column_at(0)
                    .unwrap();
                let binding = column.expression().unwrap().delete_row_reference().unwrap();
                assert!(binding.source().get_column_def().is_some());
                assert_eq!(
                    binding.version(),
                    if sql.contains("old.*") {
                        crate::sem::DeleteRowVersion::old()
                    } else {
                        crate::sem::DeleteRowVersion::new_image()
                    }
                );
            }
        }

        #[rstest::rstest]
        fn explicit_old_alias_is_ordinary_source(#[values("17", "18")] version: &str) {
            let mut context = test_helpers::catalog_for_version(version);
            let result = resolve(
                "DELETE FROM items AS old RETURNING old.id",
                &mut context,
                Dialect::postgres(),
            )
            .unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            assert!(
                result
                    .as_delete()
                    .unwrap()
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .expression()
                    .unwrap()
                    .delete_row_reference()
                    .is_none()
            );
        }
    }
    mod views {
        use crate::{
            ast::Dialect,
            names::FullName,
            parse_statements,
            sem::{CreateView, SemAst, ViewDeleteSupport, ViewLike},
            test_helpers::{create_cache_context, delete::catalog},
        };

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
    }

    use crate::{
        ast::Dialect,
        context::{BaseParseContext, ParseContext},
        names::ColumnName,
        sem::{
            DeleteStatement, PgBuiltInType,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{
            delete as test_helpers,
            delete::{analyze, catalog},
        },
    };

    #[rstest::rstest]
    #[case::plain("DELETE FROM items")]
    #[case::star("DELETE FROM items RETURNING *")]
    #[case::only("DELETE FROM ONLY items RETURNING id")]
    #[case::alias("DELETE FROM items AS i WHERE true RETURNING i.id AS removed")]
    #[case::using("DELETE FROM items USING (SELECT 2 n) s RETURNING s.n")]
    #[case::target_star("DELETE FROM items USING (SELECT 2 n) s RETURNING *")]
    #[case::source_star("DELETE FROM items USING (SELECT 2 n) s RETURNING s.*")]
    #[case::with("WITH s AS (SELECT 2 n) DELETE FROM items USING s RETURNING s.n")]
    #[case::correlated(
        "DELETE FROM items WHERE (SELECT items.id IS NULL) RETURNING (SELECT items.label)"
    )]
    #[case::cte_shadow("WITH items AS (SELECT 2 other) DELETE FROM items RETURNING id")]
    fn retains_delete_bindings(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let delete = statement.as_delete().unwrap();
        assert_eq!(delete.target().to_string(), "public.items");
        assert_eq!(delete.includes_descendants(), !sql.contains("ONLY"));
        assert!(
            delete
                .from()
                .resolve_alias(&crate::names::AliasName::from(if sql.contains("AS i ") {
                    "i"
                } else {
                    "items"
                }))
                .is_some()
        );
    }

    #[test]
    fn nested_returning_projection_fixes_its_unknown_parameter_before_outer_casts() {
        let mut context = test_helpers::catalog_with_text_integer_cast();
        let statement = test_helpers::resolve(
            "DELETE FROM items RETURNING (SELECT $1) AS label, $1::integer AS n",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraint = context
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint();
        assert!(constraint.allows(&PgBuiltInType::text()));
        assert!(!constraint.allows(&PgBuiltInType::int4()));
        assert_eq!(
            statement
                .as_delete()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::text())
        );
    }

    #[test]
    fn preserves_result_bindings_and_metadata() {
        let (statement, context) = analyze(
            "DELETE FROM items AS i RETURNING i.id AS removed, label",
            Dialect::postgres(),
        );
        let delete = statement.as_delete().unwrap();
        let column = delete.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
        assert_eq!(
            column
                .expression()
                .unwrap()
                .get_column_def()
                .unwrap()
                .name(),
            &Some(ColumnName::from("id"))
        );
        assert_eq!(
            delete
                .result_columns()
                .get_column_at(1)
                .unwrap()
                .is_not_null(),
            &Some(false)
        );
        assert!(
            context
                .get_table(&crate::names::TableName::from(
                    crate::names::FullName::from_local_name("items")
                ))
                .is_some()
        );
    }

    #[rstest::rstest]
    #[case::where_parameter("DELETE FROM items WHERE $1", true)]
    #[case::returning("DELETE FROM items RETURNING $1::integer", false)]
    #[case::with(
        "WITH s AS (SELECT $1::integer n) DELETE FROM items USING s RETURNING s.n",
        false
    )]
    #[case::nested("DELETE FROM items RETURNING (SELECT $1::integer)", false)]
    #[case::cte_returning(
        "WITH d(n) AS (DELETE FROM items RETURNING $1::integer) SELECT n FROM d",
        false
    )]
    fn infers_parameters(#[case] sql: &str, #[case] boolean: bool) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraint = context.parameter_constraints().parameter(1).unwrap();
        assert_eq!(
            constraint.constraint().allows(&PgBuiltInType::bool()),
            boolean
        );
        assert_eq!(
            constraint.constraint().allows(&PgBuiltInType::int4()),
            !boolean
        );
    }

    #[test]
    fn synthetic_parameter_conflict_retains_its_diagnostic() {
        let (_, context) = analyze(
            "DELETE FROM items WHERE $1 RETURNING $1::integer",
            Dialect::postgres(),
        );
        assert!(context.reported_problem_count() > 0);
    }

    #[test]
    fn retains_distinct_missing_and_ambiguous_lookup_outcomes() {
        let (statement, _) = analyze(
            "DELETE FROM items USING items AS other RETURNING id, absent",
            Dialect::postgres(),
        );
        let columns = statement.as_delete().unwrap().result_columns();
        let ambiguous = columns.get_column_at(0).unwrap().expression().unwrap();
        let missing = columns.get_column_at(1).unwrap().expression().unwrap();
        assert_matches::assert_matches!(
            ambiguous,
            crate::sem::SemScalarExpr::ColumnRef(
                crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_)
            )
        );
        assert_matches::assert_matches!(
            missing,
            crate::sem::SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(
                _
            ))
        );
    }

    #[rstest::rstest]
    #[case::missing_target("DELETE FROM absent")]
    #[case::unreadable("WITH d AS (DELETE FROM items) SELECT * FROM d")]
    #[case::nested("SELECT (WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d)")]
    fn retains_invalid_structure_as_diagnostics(#[case] sql: &str) {
        let mut context = catalog(Dialect::postgres());
        assert!(test_helpers::resolve(sql, &mut context, Dialect::postgres()).is_ok());
        assert!(context.reported_problem_count() > 0);
    }

    #[test]
    fn cte_aliases_and_parameters_survive_a_single_outer_inference_pass() {
        let mut context = catalog(Dialect::postgres());
        let before = parameter_resolution_inference_runs();
        let result = test_helpers::resolve("WITH s AS (SELECT $1::integer n), d(removed) AS (DELETE FROM items USING s WHERE $2 RETURNING s.n) SELECT removed FROM d", &mut context, Dialect::postgres()).unwrap();
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let query = result.into_select_statemtnt().unwrap();
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .column_name(),
            &Some(ColumnName::from("removed"))
        );
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(2)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::bool())
        );
    }

    #[test]
    fn analyzes_unused_cte_and_resets_statement_parameter_state() {
        let mut context = catalog(Dialect::postgres());
        test_helpers::resolve(
            "WITH d AS (DELETE FROM items WHERE $1) SELECT 1",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert!(context.parameter_constraints().parameter(1).is_some());
        let result = test_helpers::resolve(
            "DELETE FROM items RETURNING id",
            &mut context,
            Dialect::postgres(),
        )
        .unwrap();
        assert!(context.parameter_constraints().parameter(1).is_none());
        // Result metadata serialization follows the existing ResultColumn contract:
        // expressions are skipped, while target, scopes and column metadata persist.
        let serialized = ::serde_json::to_value(result.as_delete().unwrap()).unwrap();
        let restored: DeleteStatement = ::serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.target(), result.as_delete().unwrap().target());
        assert_eq!(
            restored
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type(),
            &Some(PgBuiltInType::int4())
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::plain("DELETE FROM items RETURNING *")]
    #[case::alias("DELETE FROM items AS i WHERE i.id IS NULL RETURNING items.id")]
    #[case::with(
        "WITH c AS (SELECT 1 n) DELETE FROM items WHERE EXISTS (SELECT n FROM c) RETURNING id"
    )]
    #[case::not_indexed("DELETE FROM items NOT INDEXED RETURNING id")]
    #[case::parameter("DELETE FROM items WHERE ?1 RETURNING ?2")]
    fn sqlite_delete(#[case] sql: &str) {
        test_helpers::sqlite_accepts(sql);
        let (statement, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(!statement.as_delete().unwrap().result_columns().is_empty());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::alias("DELETE FROM items AS i RETURNING i.id")]
    #[case::star("DELETE FROM items RETURNING items.*")]
    fn sqlite_rejects_invalid_returning(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::sqlite());
        assert!(context.reported_problem_count() > 0);
    }

    #[test]
    fn postgres_public_entry_points_agree() {
        use crate::{
            PostgresDialect,
            sem::{SemAst, analyze_delete_statement},
            syn::{ListOpt, Opt},
        };
        let sql = "DELETE FROM items WHERE $1 RETURNING id";
        let parsed = PostgresDialect::parse(sql).unwrap();
        let tokens = PostgresDialect::scan(sql).unwrap();
        let result =
            SemAst::semantic_analysis(catalog(Dialect::postgres()), parsed.clone(), &tokens)
                .unwrap();
        let delete = parsed
            .get_stmts()
            .get(0)
            .get_stmt()
            .as_delete_stmt()
            .as_inner()
            .unwrap();
        let (adapted, context) =
            analyze_delete_statement(catalog(Dialect::postgres()), &None, delete).unwrap();
        assert_eq!(result.ast().as_delete(), adapted.as_delete());
        assert_eq!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint(),
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_public_adapter_analyzes_delete() {
        let syntax =
            SqliteSynBuilder.parse_as_stmt("DELETE FROM items NOT INDEXED WHERE ?1 RETURNING id");
        let result = SemSemAst::semantic_analysis_sqlite_stmt(
            catalog(Dialect::sqlite()),
            syntax,
            &::sql_fun_core::IVec::default(),
        )
        .unwrap();
        let delete = result.ast().as_delete().unwrap();
        assert!(delete.not_indexed());
        assert_eq!(delete.result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::target("items", false)]
    #[case::other_target("other", true)]
    fn sqlite_index_hint_belongs_to_the_target(#[case] indexed_table: &str, #[case] invalid: bool) {
        use crate::{
            names::{FullName, TableName},
            sem::{CreateIndex, IndexElementCollection},
        };
        let context = catalog(Dialect::sqlite());
        let index = CreateIndex::new(
            &FullName::with_schema("public", "items_idx"),
            &TableName::from(FullName::with_schema("public", indexed_table)),
            &IndexElementCollection::from_column_names(&vec![ColumnName::from("id")].into()),
        );
        let mut context = context.apply_create_index(&index).unwrap();
        let result = test_helpers::resolve(
            "DELETE FROM items INDEXED BY items_idx RETURNING id",
            &mut context,
            Dialect::sqlite(),
        )
        .unwrap();
        assert_eq!(context.reported_problem_count() > 0, invalid);
        assert_eq!(result.as_delete().unwrap().indexed(), Some("items_idx"));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_missing_index_reports_a_structured_violation() {
        let (_, context) = analyze("DELETE FROM items INDEXED BY absent", Dialect::sqlite());
        assert_eq!(
            context.reported_problems()[0]
                .delete_problem()
                .unwrap()
                .kind(),
            &crate::sem::problem::DeleteProblemKind::index_hint()
        );
    }

    #[rstest::rstest]
    #[case::missing("DELETE FROM absent WHERE $1::boolean RETURNING absent.id", false)]
    #[case::missing_star("DELETE FROM absent RETURNING *", false)]
    #[case::found("DELETE FROM items RETURNING id", true)]
    fn retains_destination_lookup_outcome(#[case] sql: &str, #[case] found: bool) {
        let mut context = catalog(Dialect::postgres());
        let statement = test_helpers::resolve(sql, &mut context, Dialect::postgres()).unwrap();
        let delete = statement.as_delete().unwrap();
        assert_eq!(delete.target_found(), found);
        assert_eq!(context.reported_problem_count() == 0, found);
        if !found {
            assert_eq!(delete.target().to_string(), "absent");
            assert!(matches!(
                delete
                    .from()
                    .resolve_alias(&crate::names::AliasName::from("absent"))
                    .unwrap()
                    .0,
                crate::sem::DataSource::Unknown(_, _)
            ));
        }
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_delete_cte_reports_an_unsupported_operation() {
        let mut context = catalog(Dialect::sqlite());
        // The common AST can carry a DELETE CTE even though SQLite's parser rejects it.
        let statement = parse_statements(
            "WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        assert!(
            SemAst::from_ast_node(
                &statement.node,
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_ok()
        );
        assert!(context.reported_problems().iter().any(|problem| {
            problem.delete_problem().is_some_and(|problem| {
                problem.kind() == &crate::sem::problem::DeleteProblemKind::sqlite_cte()
            })
        }));
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_in(
        ast: &Delete,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let trigger_outer = if let Some(scope) = outer {
            scope.trigger_scope()
        } else {
            None
        };
        let name = FullName::from_ast_name(&ast.relation.name)?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let table = Self::from_ast_destination(
            ast,
            context,
            &requested,
            outer.is_some_and(ScalarAnalysisContext::in_rule_definition),
        )?;
        Self::validate_ast_index_hint(ast, context, &table)?;
        let with = WithClause::from_ast(&ast.with, context, outer)?;
        let alias = ast
            .relation
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = Self::from_ast_sources(ast, context, &table, &alias, &with, trigger_outer)?;
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, trigger_outer);
        let predicate = Self::from_ast_predicate(ast, context, table.name(), &scope)?;
        let returning =
            Self::from_ast_returning_scope(ast, context, &requested, &table, &alias, &scope)?;
        let mut delete = Self::new(
            table.name(),
            table.found(),
            &WithClause::ast_local_definitions(&ast.with, with),
            &from,
            &predicate,
            &returning,
        );
        delete.set_modifiers(
            table.view_support(),
            &ast.relation.indexed,
            ast.relation.not_indexed,
            ast.relation.inherit,
        );
        Ok(delete)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_destination(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        rule_definition: bool,
    ) -> Result<DeleteDestination, AnalysisError> {
        let destination = if rule_definition {
            DeleteDestination::resolve_rule_target(context, requested)
        } else {
            DeleteDestination::resolve(context, requested)
        };
        let Some(table) = destination else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            return Ok(DeleteDestination::missing(requested));
        };
        if !table.is_deletable() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::read_only_target(),
            ))?;
        }
        if !ast.returning.is_empty()
            && table
                .view_support()
                .is_some_and(|s| !s.supports_returning())
        {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::view_operation(),
            ))?;
        }
        Ok(table)
    }
}

impl DeleteStatement {
    pub(crate) fn validate_ast_index_hint(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
    ) -> Result<(), AnalysisError> {
        if let Some(index) = &ast.relation.indexed {
            let name = FullName::with_schema(
                table
                    .name()
                    .full_name()
                    .schema()
                    .map(|schema| schema.as_str())
                    .unwrap_or("main"),
                index,
            );
            if context
                .get_index(&name)
                .is_none_or(|index| !index.belongs_to(table.name()))
            {
                context.report_analysis_problem(AnalysisProblem::delete(
                    DeleteProblemKind::index_hint(),
                ))?;
            }
        }
        Ok(())
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_sources(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        // USING subqueries cannot implicitly see the DELETE target. WHERE and
        // RETURNING subqueries use the complete scope assembled below.
        let mut from = DataSource::from_ast_list_in(&ast.using, context, with, outer)?;
        let source = DataSource::resolve_relation(
            context,
            table.name().full_name(),
            alias,
            &WithClause::default(),
        )?;
        from.validate_source(context, &source)?;
        from.push(source);
        Ok(from)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_predicate(
        ast: &Delete,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<DeletePredicate>, AnalysisError> {
        if let Some(expression) = &ast.predicate
            && let Some(cursor) =
                DeleteCursor::from_ast_delete_current_of(expression, context, target)?
        {
            return Ok(Some(DeletePredicate::current_of(&cursor)));
        }
        let predicate = SemScalarExpr::from_ast_optional(&ast.predicate, context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|c| c.is_null() || c.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            context
                .report_analysis_problem(AnalysisProblem::delete(DeleteProblemKind::predicate()))?;
        }
        Ok(predicate.as_ref().map(DeletePredicate::expression))
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_returning_scope(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &DeleteDestination,
        alias: &AliasSpec,
        outer: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let with = outer.with_clause();
        let from = outer.sources();
        if context.dialect() == Dialect::sqlite() {
            let returning_alias = AliasSpec::from_local_name(&requested.full_name().local_name());
            let mut returning_from = FromClause::default();
            returning_from.push(DataSource::resolve_relation(
                context,
                table.name().full_name(),
                &returning_alias,
                &WithClause::default(),
            )?);
            let mut scope = ScalarAnalysisContext::nested(with, &returning_from, None);
            scope.set_delete_returning_target(returning_alias.name(), context);
            ResultColumnCollection::resolve_for_delete(
                &ast.returning,
                context,
                &returning_from,
                &scope,
            )
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_delete_returning_target(alias.name(), context);
            ResultColumnCollection::resolve_for_delete(&ast.returning, context, from, &scope)
        }
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        delete: &Delete,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::sqlite_cte(),
            ))?;
        }
        let mut delete = Self::from_ast_in(delete, context, Some(scope))?;
        if columns.len() > delete.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cte_aliases(),
            ))?;
        }
        delete.rename_columns(columns);
        Ok(CteStatement::from_delete(&delete))
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Delete,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(&ast.relation)?;
        if !ast.returning.is_empty() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(&ast.relation.name)?,
                "SQLite trigger DELETE does not support RETURNING",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| SemAst::delete(&statement))
            .map_err(Into::into)
    }
}
