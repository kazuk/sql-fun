use crate::{
    CacheContext,
    ast::{
        Assignment, Collate, Conflict, ConflictAction, Dialect, Expr, IndexExpression, Insert,
        Name, Query, QueryBody, document::OverridingKind,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CreateTrigger as SemCreateTrigger, CteStatement,
        DataSource, FromClause, IndirectionExpr, InsertArbiterLookup, InsertClauseSemantics,
        InsertClauseValues, InsertConflict, InsertConstraintLookup, InsertDestination,
        InsertInferenceModifiers, InsertSource, InsertStatement, InsertTarget,
        InsertTargetCollection, InsertValue, InsertValueCollection, PgBuiltInType,
        ResolutionFailure, ResultColumnCollection, ScalarAnalysisContext, SelectStatement, SemAst,
        SemScalarExpr, ViewWriteEvaluation, WithClause, parameter_resolution::ParameterResolution,
        problem::InsertProblemKind,
    },
};

impl InsertStatement {
    pub(crate) fn from_ast(
        ast: &Insert,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|value| {
            let Some(mut statement) = value else {
                return Ok(None);
            };
            let implicit_targets =
                ast.columns().is_empty() && context.dialect() == Dialect::postgres();
            let expanded = if context.parameter_inference_needed() {
                let (constraints, changed) = ParameterResolution::infer_insert(
                    &mut statement,
                    context,
                    cache,
                    implicit_targets,
                )?;
                context.set_parameter_constraints(&constraints);
                changed
            } else {
                let changed = statement.expand_source_stars(context)?;
                if changed && implicit_targets {
                    statement.truncate_implicit_targets();
                }
                changed
            };
            if expanded {
                if ResolutionFailure::finish(
                    Self::validate_ast_expanded_source(ast, &statement, context),
                    context,
                )?
                .is_none()
                {
                    return Ok(None);
                }
                statement.validate_assignments(context)?;
            }
            Ok(Some(statement))
        });
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_from_ast {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::{ColumnName, FullName as NamesFullName},
        parse_statements,
        sem::{
            CompositeFieldCollection, CompositeFieldDefinition, CreateCompositeType,
            InsertConstraintKind as SemInsertConstraintKind, InsertStatement, PgBuiltInType,
            SemAst, TypeDefinition,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{
            create_cache_context as test_helpers_create_cache_context, insert as test_helpers,
            insert::analyze,
        },
    };
    mod join_and_key_semantics {
        use crate::{
            ast::Dialect,
            names::ColumnName,
            sem::PgBuiltInType,
            test_helpers::{insert::analyze, insert_join_and_key_semantics as test_helpers},
        };

        #[rstest::rstest]
        #[case::on(
            "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 'one'::text label) b ON true) AS j RETURNING *",
            2
        )]
        #[case::using(
            "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 1 id) b USING(id)) AS j RETURNING id",
            1
        )]
        #[case::natural(
            "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a NATURAL JOIN (SELECT 1 id) b) AS j RETURNING id",
            1
        )]
        #[case::left(
            "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a LEFT JOIN (SELECT 'one'::text label) b ON false) AS j RETURNING *",
            2
        )]
        fn expands_qualified_join_star(#[case] sql: &str, #[case] width: usize) {
            let (statement, context) = analyze(sql, Dialect::postgres());
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let insert = statement.as_insert().unwrap();
            assert_eq!(insert.columns().len(), width);
            assert_eq!(insert.result_columns().len(), width);
            assert_eq!(
                insert.result_columns().get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
        }

        #[rstest::rstest]
        #[case::primary("id int4 PRIMARY KEY", true, false)]
        #[case::named_primary("id int4 CONSTRAINT key579 PRIMARY KEY", true, true)]
        #[case::unique("id int4 UNIQUE", false, false)]
        #[case::named_unique("id int4 CONSTRAINT key579 UNIQUE", false, true)]
        #[case::table_primary("id int4, CONSTRAINT key579 PRIMARY KEY(id)", true, true)]
        #[case::table_unique("id int4, CONSTRAINT key579 UNIQUE(id)", false, true)]
        fn retains_one_arbiter_and_correct_key_nullability(
            #[case] definition: &str,
            #[case] not_null: bool,
            #[case] named: bool,
            #[values("id", "*")] returning: &str,
        ) {
            let (statement, context) = test_helpers::analyze_keyed(definition, returning);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let insert = statement.as_insert().unwrap();
            let column = insert.result_columns().get_column_at(0).unwrap();
            assert_eq!(column.column_name(), &Some(ColumnName::from("id")));
            assert_eq!(column.is_not_null(), &Some(not_null));
            let arbiters = insert.conflicts()[0].arbiter().arbiters().unwrap();
            assert_eq!(arbiters.len(), 1);
            if named {
                assert_eq!(arbiters[0].constraint().unwrap().name(), "key579");
                assert!(arbiters[0].column().is_none());
            } else {
                assert_eq!(arbiters[0].column(), Some(&ColumnName::from("id")));
            }
        }
    }
    mod views {
        use crate::{
            ast::Dialect, context::ParseContext, names::ColumnName, sem::PgBuiltInType,
            test_helpers::insert_views as test_helpers,
        };

        #[rstest::rstest]
        #[case::values("INSERT INTO writable_items(id) VALUES (1) RETURNING *", 2)]
        #[case::implicit("INSERT INTO writable_items VALUES (1, 'one') RETURNING id", 1)]
        #[case::defaults("INSERT INTO writable_items DEFAULT VALUES RETURNING *", 2)]
        #[case::alias("INSERT INTO writable_items AS v(id) VALUES (1) RETURNING v.*", 2)]
        #[case::cte_shadow(
            "WITH writable_items AS (SELECT 9 AS other) INSERT INTO writable_items(id) VALUES (1) RETURNING *",
            2
        )]
        #[case::parameter("INSERT INTO writable_items(id) VALUES ($1) RETURNING id", 1)]
        #[case::select("INSERT INTO writable_items(id) SELECT id FROM items RETURNING id", 1)]
        #[case::inferred(
            "INSERT INTO writable_items(id) VALUES (1) ON CONFLICT(id) DO NOTHING RETURNING id",
            1
        )]
        #[case::nothing(
            "INSERT INTO writable_items(id) VALUES (1) ON CONFLICT DO NOTHING RETURNING *",
            2
        )]
        fn resolves_writable_view(#[case] sql: &str, #[case] width: usize) {
            let (statement, context) = test_helpers::analyze(sql, Dialect::postgres(), true, true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let insert = statement.unwrap();
            let insert = insert.as_insert().unwrap();
            assert_eq!(insert.target().to_string(), "public.writable_items");
            assert!(
                insert
                    .view_support()
                    .unwrap()
                    .can_assign(&ColumnName::from("id"))
            );
            assert_eq!(insert.result_columns().len(), width);
            assert_eq!(
                insert.result_columns().get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
            if sql.contains("$1") {
                assert!(
                    context
                        .parameter_constraints()
                        .parameter(1)
                        .unwrap()
                        .constraint()
                        .allows(&PgBuiltInType::int4())
                );
            }
        }

        #[rstest::rstest]
        #[case::read_only("INSERT INTO writable_items(id) VALUES (1)", false, false, false)]
        #[case::read_only_defaults(
            "INSERT INTO writable_items DEFAULT VALUES",
            false,
            false,
            false
        )]
        #[case::column("INSERT INTO writable_items(label) VALUES ('one')", true, false, false)]
        #[case::column_default(
            "INSERT INTO writable_items(label) VALUES (DEFAULT)",
            true,
            false,
            false
        )]
        #[case::implicit_column("INSERT INTO writable_items VALUES (1, 'one')", true, false, false)]
        #[case::writable_column("INSERT INTO writable_items(id) VALUES (1)", true, false, true)]
        #[case::omitted_columns("INSERT INTO writable_items DEFAULT VALUES", true, false, true)]
        #[case::returning_read_only_column(
            "INSERT INTO writable_items(id) VALUES (1) RETURNING label",
            true,
            false,
            true
        )]
        fn checks_view_capabilities(
            #[case] sql: &str,
            #[case] writable: bool,
            #[case] all_columns: bool,
            #[case] valid: bool,
        ) {
            let (statement, context) =
                test_helpers::analyze(sql, Dialect::postgres(), writable, all_columns);
            assert_eq!(context.reported_problem_count() == 0, valid);
            if valid {
                assert!(statement.is_ok());
            }
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::values("INSERT INTO writable_items(id) VALUES (1) RETURNING *")]
        #[case::defaults("INSERT INTO writable_items DEFAULT VALUES RETURNING id")]
        fn resolves_sqlite_trigger_view(#[case] sql: &str) {
            let oracle = test_helpers::sqlite_oracle();
            assert!(oracle.prepare(sql).is_ok());
            let (statement, context) = test_helpers::analyze(sql, Dialect::sqlite(), true, true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert!(
                statement
                    .unwrap()
                    .as_insert()
                    .unwrap()
                    .view_support()
                    .unwrap()
                    .is_writable()
            );
        }

        #[rstest::rstest]
        #[case::rule_returning(
            "INSERT INTO writable_items(id) VALUES(1) RETURNING * /* rule */",
            Dialect::postgres()
        )]
        #[case::rule_conflict(
            "INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING /* rule */",
            Dialect::postgres()
        )]
        fn rejects_unsupported_view_operation(#[case] sql: &str, #[case] dialect: Dialect) {
            let (statement, context) = test_helpers::analyze(sql, dialect, true, true);
            assert!(matches!(
                statement.unwrap(),
                crate::sem::SemAst::UnresolvedStatement
            ));
            assert!(context.reported_problems().iter().any(|problem| {
                problem.insert_problem().is_some_and(|problem| {
                    problem.kind() == crate::sem::problem::InsertProblemKind::ViewOperation
                })
            }));
        }

        #[rstest::rstest]
        #[case::explicit("INSERT INTO writable_items(id) VALUES(1) /* identity */", false)]
        #[case::default("INSERT INTO writable_items(id) VALUES(DEFAULT) /* identity */", true)]
        #[case::overriding(
            "INSERT INTO writable_items(id) OVERRIDING SYSTEM VALUE VALUES(1) /* identity */",
            true
        )]
        fn validates_catalog_view_assignment_metadata(#[case] sql: &str, #[case] valid: bool) {
            let (statement, context) = test_helpers::analyze(sql, Dialect::postgres(), true, true);
            assert!(statement.is_ok());
            assert_eq!(context.reported_problem_count() == 0, valid);
        }

        #[cfg(feature = "sqlite")]
        #[test]
        fn rejects_sqlite_view_upsert() {
            let oracle = test_helpers::sqlite_oracle();
            assert!(
                oracle
                    .prepare("INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING")
                    .is_err()
            );
            let (statement, context) = test_helpers::analyze(
                "INSERT INTO writable_items(id) VALUES(1) ON CONFLICT DO NOTHING",
                Dialect::sqlite(),
                true,
                true,
            );
            assert!(matches!(
                statement.unwrap(),
                crate::sem::SemAst::UnresolvedStatement
            ));
            assert_eq!(context.reported_problem_count(), 1);
        }
    }

    #[rstest::rstest]
    #[case::values("INSERT INTO items VALUES (1, 'one') RETURNING *")]
    #[case::partial("INSERT INTO items VALUES (1) RETURNING id")]
    #[case::named("INSERT INTO items(label, id) VALUES ('one', 1) RETURNING id AS n")]
    #[case::defaults("INSERT INTO items DEFAULT VALUES RETURNING *")]
    #[case::default_cell("INSERT INTO items VALUES (DEFAULT, 'one') RETURNING id")]
    #[case::multi_row("INSERT INTO items(id) VALUES (1), (2) RETURNING id")]
    #[case::select("INSERT INTO items(id) SELECT 1 RETURNING id")]
    #[case::select_star("INSERT INTO items SELECT * FROM items RETURNING id")]
    #[case::join_star(
        "INSERT INTO items SELECT * FROM (SELECT id FROM items) l JOIN (SELECT label FROM items) r ON true RETURNING id"
    )]
    #[case::using_star(
        "INSERT INTO items(id) SELECT * FROM (SELECT id FROM items) l JOIN (SELECT id FROM items) r USING(id) RETURNING id"
    )]
    #[case::derived_star("INSERT INTO items(id) SELECT s.* FROM (SELECT 1 AS n) s RETURNING id")]
    #[case::cte_star(
        "WITH c AS (SELECT 1 AS n) INSERT INTO items(id) SELECT * FROM c RETURNING id"
    )]
    #[case::compound("INSERT INTO items(id) SELECT 1 UNION ALL SELECT 2 RETURNING id")]
    #[case::cte_source(
        "WITH c AS (SELECT 1 AS n) INSERT INTO items(id) SELECT n FROM c RETURNING id"
    )]
    #[case::no_returning("INSERT INTO items(id) VALUES (1)")]
    #[case::alias("INSERT INTO items AS i VALUES (1, 'one') RETURNING i.id")]
    #[case::nothing("INSERT INTO items(id) VALUES (1) ON CONFLICT DO NOTHING RETURNING id")]
    fn retains_insert_target_binding(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let insert = statement.as_insert().expect("analyzed INSERT");
        assert_eq!(insert.target().to_string(), "public.items");
    }

    #[rstest::rstest]
    #[case::cte("WITH c AS (SELECT $1 n) INSERT INTO items(id) SELECT n FROM c")]
    #[case::value("INSERT INTO items(id) VALUES ($1) RETURNING id")]
    #[case::select("INSERT INTO items(id) SELECT $1 RETURNING id")]
    #[case::later_row("INSERT INTO items(id) VALUES (1), ($1) RETURNING id")]
    #[case::repeated("INSERT INTO items(id) VALUES ($1), ($1) RETURNING id")]
    #[case::compound("INSERT INTO items(id) SELECT $1 UNION ALL SELECT $1 RETURNING id")]
    fn infers_assignment_parameters(#[case] sql: &str) {
        let (_, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let constraint = context
            .parameter_constraints()
            .parameter(1)
            .expect("parameter");
        assert!(constraint.constraint().allows(&PgBuiltInType::int4()));
        assert!(!constraint.constraint().allows(&PgBuiltInType::text()));
    }

    #[rstest::rstest]
    #[case::explicit(
        "INSERT INTO expanded_items(a, b, c) SELECT ($1).*, $2 WHERE $1::public.pair IS NOT NULL"
    )]
    #[case::implicit(
        "INSERT INTO expanded_items SELECT ($1).*, $2 WHERE $1::public.pair IS NOT NULL"
    )]
    fn inferred_composite_star_assigns_expanded_insert_columns(#[case] sql: &str) {
        let mut cache_context = test_helpers_create_cache_context();
        let dialect = Dialect::postgres();
        let mut context = test_helpers::catalog(dialect);
        let pair_name = NamesFullName::with_schema("public", "pair");
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "a",
            &Some(PgBuiltInType::int4()),
            &None,
        ));
        fields.push(CompositeFieldDefinition::new(
            "b",
            &Some(PgBuiltInType::text()),
            &None,
        ));
        let pair = CreateCompositeType::new(&pair_name, &fields);
        context.set_get_type_impl_result(&pair_name, &TypeDefinition::from(&pair));
        let table_statement = parse_statements(
        "CREATE TABLE expanded_items(a integer, b text, c integer, d integer GENERATED ALWAYS AS (1) STORED)",
        dialect,
    )
    .next()
    .unwrap()
    .unwrap();
        context = SemAst::from_ast(&table_statement, context, &mut cache_context)
            .unwrap()
            .into_context();
        let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
        let result = SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert!(result.as_insert().is_some());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        assert_eq!(
            result
                .as_insert()
                .unwrap()
                .source()
                .as_query()
                .unwrap()
                .result_columns()
                .len(),
            3
        );
        let second = context.parameter_constraints().parameter(2).unwrap();
        assert!(second.constraint().allows(&PgBuiltInType::int4()));
        assert!(!second.constraint().allows(&PgBuiltInType::text()));
    }

    #[test]
    fn returning_is_exposed_through_insert_cte() {
        let (statement, context) = analyze(
            "WITH inserted(n) AS (INSERT INTO items VALUES ($1) RETURNING id) SELECT n FROM inserted",
            Dialect::postgres(),
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let query = statement.into_select_statemtnt().expect("SELECT");
        assert_eq!(
            query
                .result_columns()
                .get_result_column_def(&ColumnName::from("n"))
                .unwrap()
                .get_type(),
            Some(&PgBuiltInType::int4())
        );
        assert!(context.parameter_constraints().parameter(1).is_some());
    }

    #[test]
    fn default_values_retains_analyzed_target_without_fabricated_source() {
        let (statement, _) = analyze(
            "INSERT INTO items DEFAULT VALUES RETURNING id",
            Dialect::postgres(),
        );
        let insert = statement.as_insert().unwrap();
        assert_eq!(insert.columns().len(), 2);
        assert!(insert.source().rows().is_none());
        assert!(insert.source().as_query().is_none());
    }

    #[test]
    #[should_panic(expected = "#474")]
    fn update_action_stays_explicitly_unimplemented() {
        analyze(
            "INSERT INTO items(id) VALUES (1) ON CONFLICT(id) DO UPDATE SET id = excluded.id",
            Dialect::postgres(),
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::values("INSERT INTO items VALUES (1, 'one') RETURNING *")]
    #[case::partial("INSERT INTO items(id) VALUES (1) RETURNING id")]
    #[case::default_values("INSERT INTO items DEFAULT VALUES RETURNING id")]
    #[case::select("INSERT INTO items(id) SELECT 1 RETURNING id")]
    #[case::replace("INSERT OR REPLACE INTO items(id) VALUES (1) RETURNING id")]
    #[case::duplicate("INSERT INTO items(id,id) VALUES(1,2) RETURNING id")]
    #[case::original_qualifier("INSERT INTO items AS i(id) VALUES (1) RETURNING items.id")]
    fn sqlite_insert(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        assert!(!statement.as_insert().unwrap().result_columns().is_empty());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_implicit_columns_require_full_arity() {
        let (_, context) = analyze("INSERT INTO items VALUES (1)", Dialect::sqlite());
        assert!(context.reported_problem_count() > 0);
    }

    #[rstest::rstest]
    #[case::element("INSERT INTO arrays(a[1]) VALUES($1)")]
    #[case::parameter_index("INSERT INTO arrays(a[$2]) VALUES($1)")]
    fn resolves_array_assignment_targets(#[case] sql: &str) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        assert_eq!(
            statement.as_insert().unwrap().columns()[0]
                .destination()
                .get_type(),
            Some(PgBuiltInType::int4())
        );
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
    }

    #[rstest::rstest]
    #[case::missing_relation("INSERT INTO absent VALUES(1)", false)]
    #[case::missing_column("INSERT INTO items(absent) VALUES(1)", false)]
    #[case::unreadable_cte(
        "WITH changed AS (INSERT INTO items(id) VALUES(1)) SELECT 1 FROM changed",
        true
    )]
    fn fails_without_fabricated_bindings(#[case] sql: &str, #[case] fatal: bool) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let mut context = test_helpers::catalog(Dialect::postgres());
        let result = SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        );
        if fatal {
            assert!(result.is_err());
        } else {
            assert!(matches!(
                result.unwrap(),
                crate::sem::SemAst::UnresolvedStatement
            ));
            assert!(context.reported_problem_count() > 0);
        }
    }

    #[test]
    fn public_postgres_entry_points_share_insert_resolution() {
        use crate::{
            PostgresDialect,
            sem::{SemAst, analyze_insert_statement},
            syn::{ListOpt, Opt},
        };
        let sql = "INSERT INTO items(id) VALUES($1) RETURNING id";
        let parsed = PostgresDialect::parse(sql).unwrap();
        let tokens = PostgresDialect::scan(sql).unwrap();
        let result = SemAst::semantic_analysis(
            test_helpers::catalog(Dialect::postgres()),
            parsed.clone(),
            &tokens,
        )
        .unwrap();
        assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
        let insert = parsed
            .get_stmts()
            .get(0)
            .get_stmt()
            .as_insert_stmt()
            .as_inner()
            .unwrap();
        let (result, context) =
            analyze_insert_statement(test_helpers::catalog(Dialect::postgres()), &None, insert)
                .unwrap();
        assert_eq!(result.as_insert().unwrap().result_columns().len(), 1);
        assert!(context.parameter_constraints().parameter(1).is_some());
    }

    #[test]
    fn unreferenced_modifying_cte_is_still_analyzed_once() {
        let before = parameter_resolution_inference_runs();
        let (_, context) = analyze(
            "WITH changed AS (INSERT INTO items(id) VALUES($1)) SELECT 1",
            Dialect::postgres(),
        );
        assert_eq!(context.reported_problem_count(), 0);
        assert!(context.parameter_constraints().parameter(1).is_some());
        assert_eq!(
            crate::sem::parameter_resolution::inference_runs(),
            before + 1
        );
    }

    #[rstest::rstest]
    #[case::value("INSERT INTO items(id) VALUES($1)")]
    #[case::subscript("INSERT INTO arrays(a[$1]) VALUES(1)")]
    fn assignment_casts_do_not_become_overload_implicit_casts(#[case] sql: &str) {
        use crate::sem::{CastContext, CastDefinition};
        let mut context = test_helpers::catalog(Dialect::postgres());
        context.setup_type(PgBuiltInType::int8());
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int8(),
            &PgBuiltInType::int4(),
            Some(CastDefinition::new(CastContext::assignment())),
        );
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int8())
        );
        assert!(
            !crate::context::CastInfoRead::get_implicit_cast(
                &context,
                &PgBuiltInType::int8(),
                &PgBuiltInType::int4()
            )
            .unwrap()
            .use_in_implicit()
        );
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::values("INSERT INTO items VALUES(1,'one') RETURNING *", true)]
    #[case::short_values("INSERT INTO items VALUES(1)", false)]
    #[case::explicit_column("INSERT INTO items(id) VALUES(1) RETURNING id", true)]
    #[case::duplicates("INSERT INTO items(id,id) VALUES(1,2)", true)]
    #[case::qualified_original("INSERT INTO items AS i(id) VALUES(1) RETURNING items.id", true)]
    #[case::qualified_alias("INSERT INTO items AS i(id) VALUES(1) RETURNING i.id", false)]
    #[case::qualified_star("INSERT INTO items(id) VALUES(1) RETURNING items.*", false)]
    #[case::affinity("INSERT INTO items(id) VALUES('one') RETURNING id", true)]
    fn agrees_with_bundled_sqlite(#[case] sql: &str, #[case] accepted: bool) {
        let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
        oracle
            .execute_batch("CREATE TABLE items(id INTEGER, label TEXT)")
            .unwrap();
        assert_eq!(
            oracle.prepare(sql).is_ok(),
            accepted,
            "SQLite {}: {sql}",
            ::rusqlite::version()
        );
        let (_, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count() == 0,
            accepted,
            "{:#?}",
            context.reported_problems()
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn public_sqlite_entry_point_retains_returning_and_parameters() {
        #[cfg(feature = "sqlite")]
        use crate::{sem::SemAst, sqlite::SqliteDialect};
        let sql = "INSERT INTO items(id) VALUES(?1) RETURNING id";
        let statement = SqliteDialect::parse(sql)
            .unwrap()
            .unwrap()
            .into_plain_statement()
            .unwrap();
        let result = SemAst::semantic_analysis_sqlite_stmt(
            test_helpers::catalog(Dialect::sqlite()),
            statement,
            &SqliteDialect::scan(sql).unwrap(),
        )
        .unwrap();
        assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
    }

    #[rstest::rstest]
    #[case::unique("CREATE UNIQUE INDEX ix ON items(id)", "(id)", true)]
    #[case::duplicate("CREATE UNIQUE INDEX ix ON items(id)", "(id,id)", true)]
    #[case::nonunique("CREATE INDEX ix ON items(id)", "(id)", false)]
    #[case::wrong_column("CREATE UNIQUE INDEX ix ON items(label)", "(id)", false)]
    #[case::expression(
        "CREATE UNIQUE INDEX ix ON items((id IS NOT NULL))",
        "((id IS NOT NULL))",
        true
    )]
    #[case::expression_mismatch(
        "CREATE UNIQUE INDEX ix ON items((id IS NOT NULL))",
        "((id IS NULL))",
        false
    )]
    #[case::partial(
        "CREATE UNIQUE INDEX ix ON items(id) WHERE id IS NOT NULL",
        "(id) WHERE id IS NOT NULL",
        true
    )]
    #[case::partial_without_predicate(
        "CREATE UNIQUE INDEX ix ON items(id) WHERE id IS NOT NULL",
        "(id)",
        false
    )]
    fn validates_conflict_index(#[case] index: &str, #[case] target: &str, #[case] accepted: bool) {
        let mut cache_context = test_helpers_create_cache_context();
        let dialect = Dialect::postgres();
        let mut context = SemAst::from_ast(
            &parse_statements(index, dialect).next().unwrap().unwrap(),
            test_helpers::catalog(dialect),
            &mut cache_context,
        )
        .unwrap()
        .into_context();
        assert_eq!(context.reported_problem_count(), 0);
        let sql = format!("INSERT INTO items(id) VALUES(1) ON CONFLICT {target} DO NOTHING");
        let statement = parse_statements(&sql, dialect).next().unwrap().unwrap();
        let result = SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count() == 0,
            accepted,
            "{:#?}",
            context.reported_problems()
        );
        assert_eq!(result.as_insert().unwrap().conflicts().len(), 1);
        let lookup = result.as_insert().unwrap().conflicts()[0].arbiter();
        assert_eq!(lookup.is_not_found(), !accepted);
        if accepted {
            let index = lookup.arbiters().unwrap()[0].index_name().unwrap();
            assert_eq!(index, &crate::names::FullName::with_schema("public", "ix"));
        }
    }

    #[test]
    fn returning_retains_expression_alias_and_nullability() {
        let (statement, context) = analyze(
            "INSERT INTO items AS i VALUES(1,'x') RETURNING i.label AS name, i.id",
            Dialect::postgres(),
        );
        assert_eq!(context.reported_problem_count(), 0);
        let columns = statement.as_insert().unwrap().result_columns();
        let label = columns.get_column_at(0).unwrap();
        assert_eq!(label.column_name(), &Some(ColumnName::from("name")));
        assert_eq!(label.col_type(), &Some(PgBuiltInType::text()));
        assert_eq!(label.is_not_null(), &Some(false));
        assert!(label.expression().is_some());
        assert_eq!(columns.get_column_at(1).unwrap().is_not_null(), &Some(true));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    #[should_panic(expected = "#474")]
    fn sqlite_later_update_action_remains_explicitly_unimplemented() {
        analyze(
            "INSERT INTO items VALUES(1,'x') ON CONFLICT(id) DO NOTHING ON CONFLICT DO UPDATE SET label='x'",
            Dialect::sqlite(),
        );
    }

    #[rstest::rstest]
    #[case::collation(r#"label COLLATE "C""#, false, "C")]
    #[case::expression_collation(r#"(label COLLATE "C")"#, false, "C")]
    #[case::qualified_collation("label COLLATE custom.casefold", false, "custom.casefold")]
    #[case::operator_class("id int4_ops", true, "int4_ops")]
    #[case::qualified_operator_class("id custom.integer_ops", true, "custom.integer_ops")]
    fn binds_conflict_modifier_through_context(
        #[case] target: &str,
        #[case] operator: bool,
        #[case] requested: &str,
        #[values("found", "missing", "ambiguous")] outcome: &str,
    ) {
        let (statement, context, expected) = test_helpers::modifier_analysis(
            target,
            operator,
            requested,
            outcome,
            Dialect::postgres(),
        );
        let conflict = &statement.as_insert().unwrap().conflicts()[0];
        let modifier = &conflict.modifiers()[0];
        let lookup = if operator {
            modifier.operator_class()
        } else {
            modifier.collation()
        }
        .unwrap();
        assert_eq!(lookup, &expected);
        assert_eq!(
            lookup.requested(),
            &requested
                .parse::<crate::names::FullName>()
                .expect("logical fixture name")
        );
        if outcome == "found" {
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:#?}",
                context.reported_problems()
            );
            assert_eq!(
                conflict.arbiter().arbiters().unwrap()[0]
                    .index_name()
                    .unwrap()
                    .to_string(),
                "public.mock_arbiter"
            );
            assert!(lookup.resolved_name().unwrap().schema().is_some());
        } else {
            assert!(conflict.arbiter().is_not_found());
            let kind = context.reported_problems()[0]
                .insert_problem()
                .unwrap()
                .kind();
            if outcome == "missing" {
                assert!(lookup.is_missing());
                assert_eq!(
                    kind,
                    crate::sem::problem::InsertProblemKind::ConflictNameMissing
                );
            } else {
                assert_eq!(lookup.ambiguous_candidates().unwrap().len(), 2);
                assert_eq!(
                    kind,
                    crate::sem::problem::InsertProblemKind::ConflictNameAmbiguous
                );
            }
        }
        let json = ::serde_json::to_string(statement.as_insert().unwrap()).unwrap();
        let roundtrip: InsertStatement = ::serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.conflicts()[0].modifiers(), conflict.modifiers());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::lower("label COLLATE nocase", "nocase")]
    #[case::parenthesized("(label COLLATE nocase)", "nocase")]
    #[case::mixed_case("label COLLATE NoCaSe", "NoCaSe")]
    fn sqlite_conflict_collation_uses_connection_catalog(
        #[case] target: &str,
        #[case] requested: &str,
        #[values("found", "missing", "ambiguous")] outcome: &str,
    ) {
        let (statement, context, expected) =
            test_helpers::modifier_analysis(target, false, requested, outcome, Dialect::sqlite());
        let conflict = &statement.as_insert().unwrap().conflicts()[0];
        assert_eq!(conflict.modifiers()[0].collation(), Some(&expected));
        assert!(conflict.modifiers()[0].operator_class().is_none());
        if outcome == "found" {
            assert_eq!(context.reported_problem_count(), 0);
            assert!(expected.resolved_name().unwrap().schema().is_none());
        } else {
            assert!(conflict.arbiter().is_not_found());
            assert!(context.reported_problem_count() > 0);
        }
    }

    #[test]
    fn preserves_modifier_positions_and_looks_up_both_names() {
        use crate::{
            names::FullName,
            sem::{CatalogNameLookup, InsertArbiter, InsertInferenceModifiers},
        };
        let dialect = Dialect::postgres();
        let mut context = test_helpers::catalog(dialect);
        let collation_name = FullName::from_local_name("C");
        let class_name = FullName::from_local_name("text_ops");
        let collation =
            CatalogNameLookup::found(&collation_name, &FullName::with_schema("pg_catalog", "C"));
        let operator_class = CatalogNameLookup::found(
            &class_name,
            &FullName::with_schema("pg_catalog", "text_ops"),
        );
        context.set_collation_lookup(&collation_name, &collation);
        context.set_operator_class_lookup(&class_name, &operator_class);
        let expected = vec![
            InsertInferenceModifiers::new(None, None),
            InsertInferenceModifiers::new(Some(collation), Some(operator_class)),
        ];
        context.expect_conflict_modifiers(
            &expected,
            &[InsertArbiter::index(&FullName::with_schema(
                "public", "both",
            ))],
        );
        let result = SemAst::from_ast_node(parse_statements(
        r#"INSERT INTO items VALUES(1,'x') ON CONFLICT(id, label COLLATE "C" text_ops) DO NOTHING"#,
        dialect,
    )
    .next()
    .unwrap()
    .unwrap()
    .node(), &mut context, &mut test_helpers_create_cache_context())
    .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(
            result.as_insert().unwrap().conflicts()[0].modifiers(),
            expected
        );
    }

    #[test]
    fn retains_failed_inner_collation_even_when_outer_collation_is_bound() {
        use crate::{names::FullName, sem::CatalogNameLookup};
        let dialect = Dialect::postgres();
        let mut context = test_helpers::catalog(dialect);
        let missing = FullName::from_local_name("absent579");
        let outer = FullName::from_local_name("C");
        let missing_lookup = CatalogNameLookup::missing(&missing);
        let outer_lookup =
            CatalogNameLookup::found(&outer, &FullName::with_schema("pg_catalog", "C"));
        context.set_collation_lookup(&missing, &missing_lookup);
        context.set_collation_lookup(&outer, &outer_lookup);
        // Any attempt to infer an arbiter from these failed bindings is a bug.
        context.expect_conflict_modifiers(&[], &[]);
        let result = SemAst::from_ast_node(parse_statements(
        r#"INSERT INTO items VALUES(1,'x') ON CONFLICT ((label COLLATE absent579) COLLATE "C") DO NOTHING"#,
        dialect,
    ).next().unwrap().unwrap().node(), &mut context, &mut test_helpers_create_cache_context()).unwrap();
        let conflict = &result.as_insert().unwrap().conflicts()[0];
        assert_eq!(
            conflict.modifiers()[0].collation_references(),
            &[missing_lookup, outer_lookup.clone()]
        );
        assert_eq!(conflict.modifiers()[0].collation(), Some(&outer_lookup));
        assert!(conflict.arbiter().is_not_found());
        assert_eq!(
            context.reported_problems()[0]
                .insert_problem()
                .unwrap()
                .kind(),
            crate::sem::problem::InsertProblemKind::ConflictNameMissing
        );
    }

    #[test]
    fn resolved_modifier_does_not_guarantee_an_eligible_arbiter() {
        let (statement, context, lookup) = test_helpers::modifier_analysis(
            "id int4_ops",
            true,
            "int4_ops",
            "unmatched",
            Dialect::postgres(),
        );
        let conflict = &statement.as_insert().unwrap().conflicts()[0];
        assert!(lookup.resolved_name().is_some());
        assert_eq!(conflict.modifiers()[0].operator_class(), Some(&lookup));
        assert!(conflict.arbiter().is_not_found());
        assert_eq!(
            context.reported_problems()[0]
                .insert_problem()
                .unwrap()
                .kind(),
            crate::sem::problem::InsertProblemKind::ConflictTarget
        );
    }

    #[rstest::rstest]
    #[case::primary("PRIMARY KEY", crate::sem::InsertConstraintKind::PrimaryKey)]
    #[case::unique("UNIQUE", crate::sem::InsertConstraintKind::Unique)]
    fn resolves_named_constraint_identity(
        #[case] kind: &str,
        #[case] expected: SemInsertConstraintKind,
    ) {
        let mut cache_context = test_helpers_create_cache_context();
        let dialect = Dialect::postgres();
        let ddl = format!("CREATE TABLE keyed(id int4, CONSTRAINT key473 {kind}(id))");
        let mut context = SemAst::from_ast(
            &parse_statements(&ddl, dialect).next().unwrap().unwrap(),
            test_helpers::catalog(dialect),
            &mut cache_context,
        )
        .unwrap()
        .into_context();
        let statement = parse_statements(
            "INSERT INTO keyed VALUES(1) ON CONFLICT ON CONSTRAINT key473 DO NOTHING",
            dialect,
        )
        .next()
        .unwrap()
        .unwrap();
        let result = SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let lookup = result.as_insert().unwrap().conflicts()[0]
            .arbiter()
            .constraint()
            .unwrap();
        assert_eq!(lookup.kind(), expected);
        assert_eq!(lookup.name(), "key473");
        assert_eq!(lookup.table().to_string(), "public.keyed");
        let returning = SemAst::from_ast_node(
            parse_statements("INSERT INTO keyed VALUES(2) RETURNING id", dialect)
                .next()
                .unwrap()
                .unwrap()
                .node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        let nullability = returning
            .as_insert()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .is_not_null();
        if kind == "PRIMARY KEY" {
            assert_eq!(nullability, &Some(true));
        }
    }

    #[test]
    fn retains_failed_constraint_lookup_and_structured_problem() {
        let (statement, context) = analyze(
            "INSERT INTO items VALUES(1,'x') ON CONFLICT ON CONSTRAINT absent DO NOTHING",
            Dialect::postgres(),
        );
        let lookup = statement.as_insert().unwrap().conflicts()[0]
            .arbiter()
            .constraint()
            .unwrap();
        assert_eq!(lookup.kind(), crate::sem::InsertConstraintKind::NotFound);
        assert_eq!(lookup.name(), "absent");
        assert_eq!(
            context.reported_problems()[0]
                .insert_problem()
                .unwrap()
                .kind(),
            crate::sem::problem::InsertProblemKind::ConflictConstraint
        );
    }

    #[test]
    fn retains_multiple_unnamed_table_keys_without_fabricating_names() {
        let mut cache_context = test_helpers_create_cache_context();
        let dialect = Dialect::postgres();
        let mut context = SemAst::from_ast(
            &parse_statements(
                "CREATE TABLE keyed(id int4, label text, UNIQUE(id), UNIQUE(label))",
                dialect,
            )
            .next()
            .unwrap()
            .unwrap(),
            test_helpers::catalog(dialect),
            &mut cache_context,
        )
        .unwrap()
        .into_context();
        let statement = parse_statements(
            "INSERT INTO keyed VALUES(1,'x') ON CONFLICT(label) DO NOTHING",
            dialect,
        )
        .next()
        .unwrap()
        .unwrap();
        let result = SemAst::from_ast_node(
            statement.node(),
            &mut context,
            &mut test_helpers_create_cache_context(),
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:#?}",
            context.reported_problems()
        );
        let arbiters = result.as_insert().unwrap().conflicts()[0]
            .arbiter()
            .arbiters()
            .unwrap();
        assert_eq!(
            arbiters[0].anonymous_key_columns(),
            Some([ColumnName::from("label")].as_slice())
        );
        assert!(arbiters[0].constraint().is_none());
    }

    #[test]
    fn nested_insert_cte_reports_a_query_problem() {
        let mut context = test_helpers::catalog(Dialect::postgres());
        let statement = parse_statements("SELECT * FROM (WITH changed AS (INSERT INTO items(id) VALUES(1) RETURNING id) SELECT id FROM changed) s", Dialect::postgres()).next().unwrap().unwrap();
        assert!(
            SemAst::from_ast_node(
                statement.node(),
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_ok()
        );
        assert!(context.reported_problem_count() > 0);
    }
}

impl InsertStatement {
    pub(crate) fn validate_ast_expanded_source(
        ast: &Insert,
        statement: &Self,
        context: &mut impl ParseContext,
    ) -> Result<(), ResolutionFailure> {
        let source = statement.source();
        source.validate_width(context, statement.columns().len())?;
        for (index, target) in statement.columns().iter().enumerate() {
            let defaults_only = source.rows().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get(index).is_some_and(InsertValue::is_default))
            });
            target.validate_value(
                context,
                defaults_only,
                ast.overriding()
                    .as_ref()
                    .is_some_and(InsertClauseSemantics::ast_overrides_identity),
            )?;
            if let Some(name) = target.column().name()
                && statement
                    .view_support()
                    .is_some_and(|support| !support.can_assign(name))
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::read_only_target(),
                    &format!("view column {name} does not accept assignments"),
                ))?;
            }
        }
        if ast.columns().is_empty()
            && context.dialect() == Dialect::postgres()
            && let Some(support) = statement.view_support()
        {
            let mut seen = Vec::new();
            for target in statement.columns() {
                let name = target.column().name().as_ref().expect("named destination");
                let key = support.assignment_column(name);
                if seen.contains(&key) {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        InsertProblemKind::duplicate_target(),
                        "underlying column is assigned more than once",
                    ))?;
                }
                seen.push(key);
            }
        }
        Ok(())
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_in(
        ast: &Insert,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Option<Self>, AnalysisError> {
        let result = Self::from_ast_definition(ast, context, outer);
        ResolutionFailure::finish(result, context)
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_definition(
        ast: &Insert,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, ResolutionFailure> {
        if let Some(conflict) = &ast.conflict() {
            InsertConflict::require_ast_conflict_action(conflict);
        }
        let name = FullName::from_ast_name(ast.relation().name())?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let destination = if outer.is_some_and(ScalarAnalysisContext::in_rule_definition) {
            InsertDestination::resolve_rule_target(context, &requested)?
        } else {
            InsertDestination::resolve(context, &requested)?
        };
        let Some(table) = destination else {
            if context
                .resolve_relation_name(requested.full_name())
                .is_none()
            {
                context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            }
            return Err(ResolutionFailure::reported());
        };
        if !table.is_writable() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::read_only_target(),
                "destination view does not support INSERT",
            ))?;
            return Err(ResolutionFailure::reported());
        }
        if let Some(support) = table.view_support() {
            if !ast.returning().is_empty() && !support.supports_returning() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::view_operation(),
                    "destination view does not support INSERT RETURNING",
                ))?;
                return Err(ResolutionFailure::reported());
            }
            if ast.conflict().is_some() && !support.supports_on_conflict() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::view_operation(),
                    "destination view does not support ON CONFLICT",
                ))?;
                return Err(ResolutionFailure::reported());
            }
        }
        let name = table.name().clone();
        let with = WithClause::from_ast(ast.with(), context, outer)?;
        let empty = FromClause::default();
        let source_scope = ScalarAnalysisContext::rule_action_source(&with, &empty, outer);
        let source = ast
            .source()
            .as_ref()
            .map(|query| InsertSource::from_ast(query, context, &source_scope))
            .transpose()?
            .unwrap_or_else(InsertSource::default_values);
        let alias = ast
            .relation()
            .alias()
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| {
                AliasSpec::from_local_name(
                    &FullName::from_ast_name(ast.relation().name())
                        .expect("target name")
                        .local_name(),
                )
            });
        let mut from = FromClause::default();
        from.push(DataSource::resolve_relation(
            context,
            name.full_name(),
            &alias,
            &WithClause::default(),
        )?);
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, outer);
        let implicit_width = if context.dialect() == Dialect::postgres() {
            if ast.source().is_some() {
                if source
                    .as_query()
                    .is_some_and(SelectStatement::has_pending_stars)
                {
                    None
                } else {
                    Some(source.width())
                }
            } else {
                None
            }
        } else {
            None
        };
        let pending_source = source
            .as_query()
            .is_some_and(SelectStatement::has_pending_stars);
        let columns = InsertTargetCollection::resolve(
            context,
            ast.columns(),
            &InsertTargetBinding::new(&table, &alias, &scope),
            &scope,
            InsertTargetMode::new(ast.source().is_some(), implicit_width, pending_source),
        )?;
        columns.validate_source(
            context,
            &source,
            ast.source().is_some(),
            ast.overriding().as_ref(),
        )?;
        let returning_alias = if context.dialect() == Dialect::sqlite() {
            AliasSpec::from_local_name(
                &FullName::from_ast_name(ast.relation().name())?.local_name(),
            )
        } else {
            alias.clone()
        };
        let mut returning_from = FromClause::default();
        returning_from.push(DataSource::resolve_relation(
            context,
            name.full_name(),
            &returning_alias,
            &WithClause::default(),
        )?);
        let rule_returning = if let Some(scope) = outer {
            scope.insert_returning_outer(context)
        } else {
            None
        };
        let mut returning_scope =
            ScalarAnalysisContext::nested(&with, &returning_from, rule_returning.as_ref());
        returning_scope.set_postgres_returning(context);
        let returning = ResultColumnCollection::resolve_for_insert(
            ast.returning(),
            context,
            &returning_scope,
            &table,
            &returning_alias,
        )?;
        let conflicts = ast
            .conflict()
            .as_ref()
            .map(|conflict| {
                InsertConflict::from_ast_insert(conflict, context, &scope, &table, &name)
            })
            .transpose()?
            .unwrap_or_default();
        let mut insert = Self::new(
            &name,
            &columns,
            &source,
            &WithClause::ast_local_definitions(ast.with(), with),
            &returning,
        );
        let overriding = if let Some(mode) = &ast.overriding() {
            InsertClauseSemantics::ast_insert_mode(mode)
        } else {
            None
        };
        insert.set_view_support(table.view_support());
        insert.set_modifiers(conflicts, overriding, ast.or_conflict().clone());
        if !insert.source_has_pending_stars() {
            insert.validate_assignments(context)?;
        }
        Ok(insert)
    }
}

/// Resolve one INSERT-like clause using independent destination and value scopes.
impl InsertClauseSemantics {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        values: Option<&[Expr]>,
        value_scope: &ScalarAnalysisContext<'_>,
        overriding: Option<&OverridingKind>,
    ) -> Result<Self, ResolutionFailure> {
        let implicit_width = values.map(|values| values.len());
        let targets = InsertTargetCollection::resolve(
            context,
            assignments,
            destination,
            value_scope,
            InsertTargetMode::new(values.is_some(), implicit_width, false),
        )?;
        let values = values
            .map(|values| InsertValueCollection::resolve(values, context, value_scope))
            .transpose()?;
        let clause = Self::new(&targets, &InsertClauseValues::from_values(values));
        clause.validate(context, overriding)?;
        Ok(clause)
    }
}

impl InsertClauseSemantics {
    fn validate(
        &self,
        context: &mut impl ParseContext,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        if let Some(values) = self.values().values() {
            self.targets()
                .validate_values(context, values, overriding)?;
        }
        Ok(())
    }
}

/// Destination-only bindings for an INSERT target list.
pub(crate) struct InsertTargetBinding<'a, 'scope> {
    table: &'a InsertDestination,
    scope: &'a ScalarAnalysisContext<'scope>,
    alias: &'a AliasSpec,
}

impl<'a, 'scope> InsertTargetBinding<'a, 'scope> {
    pub(crate) const fn new(
        table: &'a InsertDestination,
        alias: &'a AliasSpec,
        scope: &'a ScalarAnalysisContext<'scope>,
    ) -> Self {
        Self {
            table,
            scope,
            alias,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InsertTargetMode {
    source_is_present: bool,
    implicit_width: Option<usize>,
    pending_source: bool,
}

impl InsertTargetMode {
    pub(crate) const fn new(
        source_is_present: bool,
        implicit_width: Option<usize>,
        pending_source: bool,
    ) -> Self {
        Self {
            source_is_present,
            implicit_width,
            pending_source,
        }
    }
}

/// Resolve destination bindings for an INSERT clause.
impl InsertTargetCollection {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        assignments: &[Assignment],
        destination: &InsertTargetBinding<'_, '_>,
        expression_scope: &ScalarAnalysisContext<'_>,
        mode: InsertTargetMode,
    ) -> Result<Self, ResolutionFailure> {
        let table = destination.table;
        let alias = destination.alias;
        let scope = destination.scope;
        let mut columns = Self::default();
        if assignments.is_empty() {
            let mut index = 0;
            while let Some(column) = table.get_column_at(index) {
                index += 1;
                if context.dialect() == Dialect::sqlite()
                    && column
                        .constraints()
                        .iter()
                        .any(|constraint| constraint.as_generated().is_some())
                {
                    continue;
                }
                let name = column.name().as_ref().expect("catalog column name");
                let destination = scope.resolve_column(context, Some(alias.name()), name)?.0;
                columns.push(InsertTarget::new(column, destination));
            }
            // PostgreSQL permits the first N implicit columns, unlike SQLite.
            if context.dialect() == Dialect::postgres()
                && let Some(width) = mode.implicit_width
            {
                columns.truncate(width);
            }
            if context.dialect() == Dialect::postgres()
                && mode.source_is_present
                && !mode.pending_source
                && let Some(support) = table.view_support()
            {
                let mut seen = Vec::new();
                for target in columns.as_slice() {
                    let name = target.column().name().as_ref().expect("named destination");
                    let key = support.assignment_column(name);
                    if seen.contains(&key) {
                        context.report_analysis_problem(AnalysisProblem::insert(
                            InsertProblemKind::duplicate_target(),
                            "underlying column is assigned more than once",
                        ))?;
                    }
                    seen.push(key);
                }
            }
        } else {
            let mut seen: Vec<&Assignment> = Vec::new();
            for assignment in assignments {
                if context.dialect() == Dialect::postgres()
                    && seen.iter().any(|previous| {
                        Self::ast_overlaps_mapped(assignment, previous, |name| {
                            table.view_support().map_or_else(
                                || name.clone(),
                                |support| support.assignment_column(name).clone(),
                            )
                        })
                    })
                {
                    context.report_analysis_problem(AnalysisProblem::insert(
                        InsertProblemKind::duplicate_target(),
                        &format!("column {} is assigned more than once", assignment.name()),
                    ))?;
                }
                seen.push(assignment);
                let column_name = ColumnName::from(assignment.name().as_str());
                let Some(column) = table.get_column_def(&column_name) else {
                    context.report_analysis_problem(AnalysisProblem::column_not_found_in_table(
                        table.name(),
                        &column_name,
                    ))?;
                    return Err(ResolutionFailure::reported());
                };
                let destination = scope
                    .resolve_column(context, Some(alias.name()), &column_name)?
                    .0;
                let destination = IndirectionExpr::from_ast_sequence(
                    assignment.indirection(),
                    context,
                    expression_scope,
                    destination,
                )?;
                columns.push(InsertTarget::new(column, destination));
            }
        }
        for target in columns.as_slice() {
            if mode.source_is_present
                && !mode.pending_source
                && let Some(name) = target.column().name()
                && !table.can_assign(name)
            {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::read_only_target(),
                    &format!("view column {name} does not accept assignments"),
                ))?;
            }
        }
        Ok(columns)
    }

    fn validate_values(
        &self,
        context: &mut impl ParseContext,
        values: &InsertValueCollection,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        validate_insert_width(
            context,
            values.len(),
            self.as_slice().len(),
            "INSERT source and destination column counts differ",
        )?;
        for (index, target) in self.as_slice().iter().enumerate() {
            target.validate_value(
                context,
                values
                    .as_slice()
                    .get(index)
                    .is_some_and(InsertValue::is_default),
                overriding.is_some_and(InsertClauseSemantics::ast_overrides_identity),
            )?;
        }
        Ok(())
    }

    fn validate_source(
        &self,
        context: &mut impl ParseContext,
        source: &InsertSource,
        source_is_present: bool,
        overriding: Option<&OverridingKind>,
    ) -> Result<(), ResolutionFailure> {
        if !source_is_present {
            return Ok(());
        }
        if source
            .as_query()
            .is_some_and(SelectStatement::has_pending_stars)
        {
            return Ok(());
        }
        source.validate_width(context, self.as_slice().len())?;
        for (index, target) in self.as_slice().iter().enumerate() {
            let defaults_only = source.rows().is_some_and(|rows| {
                rows.iter()
                    .all(|row| row.get(index).is_some_and(InsertValue::is_default))
            });
            target.validate_value(
                context,
                defaults_only,
                overriding.is_some_and(InsertClauseSemantics::ast_overrides_identity),
            )?;
        }
        Ok(())
    }
}

impl InsertValueCollection {
    pub(crate) fn resolve(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        let mut values = Self::default();
        for expression in expressions {
            values.push(InsertValue::from_ast(expression, context, scope)?);
        }
        Ok(values)
    }
}

impl InsertTarget {
    fn validate_value(
        &self,
        context: &mut impl ParseContext,
        defaults_only: bool,
        overrides_identity: bool,
    ) -> Result<(), ResolutionFailure> {
        let constraints = self.column().constraints();
        if constraints
            .iter()
            .any(|constraint| constraint.as_generated().is_some())
            && !defaults_only
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::generated_target(),
                "generated column accepts only DEFAULT",
            ))?;
        }
        if constraints
            .iter()
            .filter_map(|constraint| constraint.as_identity())
            .any(|identity| identity.is_always())
            && !defaults_only
            && !overrides_identity
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::identity_target(),
                "GENERATED ALWAYS identity requires DEFAULT or OVERRIDING",
            ))?;
        }
        Ok(())
    }
}

impl InsertSource {
    pub(crate) fn validate_width(
        &self,
        context: &mut impl ParseContext,
        width: usize,
    ) -> Result<(), ResolutionFailure> {
        validate_insert_width(
            context,
            self.width(),
            width,
            "INSERT source and destination column counts differ",
        )?;
        if let Some(query) = self.as_query()
            && !query.has_width(width)
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::arity(),
                "INSERT query branches must match destination column count",
            ))?;
        }
        if let Some(rows) = self.rows() {
            for row in rows {
                validate_insert_width(
                    context,
                    row.len(),
                    width,
                    "INSERT VALUES rows must match the destination column count",
                )?;
            }
        }
        Ok(())
    }
}

fn validate_insert_width(
    context: &mut impl ParseContext,
    actual: usize,
    expected: usize,
    message: &str,
) -> Result<(), ResolutionFailure> {
    if actual != expected {
        context.report_analysis_problem(AnalysisProblem::insert(
            InsertProblemKind::arity(),
            message,
        ))?;
    }
    Ok(())
}

impl InsertSource {
    pub(crate) fn from_ast(
        ast: &Query,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        if let QueryBody::Values(rows) = &ast.body()
            && ast.with().entries().is_empty()
            && ast.order().is_empty()
            && ast.limit().is_none()
            && ast.offset().is_none()
            && ast.locks().is_empty()
        {
            let mut source = Self::empty_values();
            for row in rows {
                let values = InsertValueCollection::resolve(row, context, scope)?;
                source.push_values(&values);
            }
            return Ok(source);
        }
        Ok(Self::query(SelectStatement::from_ast_in(
            ast,
            context,
            Some(scope),
        )?))
    }
}

impl InsertValue {
    pub(crate) fn from_ast(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Default => Ok(Self::default_value()),
            _ => Ok(Self::expression(SemScalarExpr::from_ast_in(
                ast, context, scope,
            )?)),
        }
    }
}

impl InsertTargetCollection {
    pub(crate) fn ast_overlaps_mapped(
        ast: &Assignment,
        other: &Assignment,
        map: impl Fn(&ColumnName) -> ColumnName,
    ) -> bool {
        let same = map(&ColumnName::from(ast.name().as_str()))
            == map(&ColumnName::from(other.name().as_str()));
        same && (ast.indirection().is_empty() || other.indirection().is_empty())
    }
}

impl InsertConflict {
    pub(crate) fn require_ast_action(ast: &ConflictAction) {
        if matches!(ast, ConflictAction::Update) {
            todo!("ON CONFLICT DO UPDATE semantics are tracked in #474");
        }
    }
}

impl InsertConflict {
    pub(crate) fn require_ast_conflict_action(ast: &Conflict) {
        if let Some(action) = &ast.action() {
            Self::require_ast_action(action);
        }
        if let Some(next) = &ast.next() {
            Self::require_ast_conflict_action(next.as_ref());
        }
    }
}

impl InsertConflict {
    pub(crate) fn from_ast_insert(
        ast: &Conflict,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        table: &InsertDestination,
        name: &TableName,
    ) -> Result<Vec<Self>, ResolutionFailure> {
        if context.dialect() == Dialect::sqlite()
            && ast.next().is_some()
            && ast.inference().is_empty()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::conflict_chain(),
                "only the last SQLite UPSERT clause may omit its conflict target",
            ))?;
        }
        let mut inference = Vec::new();
        for item in ast.inference() {
            if let Some(name) = item.name() {
                inference.push(
                    scope
                        .resolve_column(context, None, &ColumnName::from(name.as_str()))?
                        .0
                        .mark_unqualified_column(),
                );
            }
            if let Some(expression) = item.expression() {
                inference.push(SemScalarExpr::from_ast_insert_inference(
                    expression, context, scope,
                )?);
            }
        }
        let predicate =
            SemScalarExpr::from_ast_optional(ast.inference_predicate(), context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|value| value.is_null() || value.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::conflict_predicate(),
                "ON CONFLICT predicate must have boolean type",
            ))?;
        }

        let mut conflict = Self::new(inference, predicate, ast.constraint().as_deref());
        let mut modifiers = Vec::new();
        for item in ast.inference() {
            modifiers.push(InsertInferenceModifiers::from_ast_insert_modifiers(
                item, context,
            )?);
        }
        conflict.set_modifiers(modifiers);
        if let Some(constraint) = &ast.constraint() {
            let lookup =
                InsertConstraintLookup::new(name, constraint, table.constraint(constraint));
            if !lookup.is_usable() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::conflict_constraint(),
                    "ON CONFLICT constraint is not a unique or primary key constraint of the destination",
                ))?;
            }
            conflict.set_arbiter(InsertArbiterLookup::named(&lookup));
        }
        if !ast.inference().is_empty() {
            let arbiters = if conflict.modifiers_are_bound() {
                match if context
                    .get_view(&ViewName::from_full_name(name.full_name()))
                    .is_some()
                {
                    ViewWriteEvaluation::new(context)
                        .infer_arbiters(name, &conflict)?
                        .ok_or_else(ResolutionFailure::reported)?
                } else {
                    context.infer_conflict_arbiters(name, &conflict)?
                }
                .into_result()
                {
                    Ok(arbiters) => arbiters,
                    Err(problem) => {
                        context.report_analysis_problem(*problem)?;
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
            conflict.set_arbiter(InsertArbiterLookup::inferred(&arbiters));
            if arbiters.is_empty() && conflict.modifiers_are_bound() {
                context.report_analysis_problem(AnalysisProblem::insert(
                    InsertProblemKind::conflict_target(),
                    "ON CONFLICT target has no matching unique key",
                ))?;
            }
        }
        let mut conflicts = vec![conflict];
        if let Some(next) = &ast.next() {
            conflicts.extend(Self::from_ast_insert(
                next.as_ref(),
                context,
                scope,
                table,
                name,
            )?);
        }
        Ok(conflicts)
    }
}

impl InsertClauseSemantics {
    pub(crate) fn ast_insert_mode(ast: &OverridingKind) -> Option<String> {
        match ast {
            OverridingKind::UserValue => Some("user".to_owned()),
            OverridingKind::SystemValue => Some("system".to_owned()),
            _ => None,
        }
    }
}

impl InsertClauseSemantics {
    pub(crate) const fn ast_overrides_identity(ast: &OverridingKind) -> bool {
        matches!(ast, OverridingKind::UserValue | OverridingKind::SystemValue)
    }
}

impl InsertInferenceModifiers {
    pub(crate) fn from_ast_insert_modifiers(
        ast: &IndexExpression,
        context: &mut impl ParseContext,
    ) -> Result<Self, ResolutionFailure> {
        let mut names = Vec::new();
        if let Some(expression) = &ast.expression() {
            Self::collect_ast_collations(expression, &mut names);
        }
        if !ast.collation().parts().is_empty() {
            names.push(ast.collation());
        }
        let mut collations = Vec::new();
        for name in names {
            let lookup = context.lookup_collation(&FullName::from_ast_name(name)?);
            lookup.report_insert_problem(context)?;
            collations.push(lookup);
        }
        let operator_class = if ast.operator_class().parts().is_empty() {
            None
        } else {
            let lookup = context
                .lookup_operator_class(&FullName::from_ast_name(ast.operator_class())?, "btree");
            lookup.report_insert_problem(context)?;
            Some(lookup)
        };
        Ok(Self::from_lookups(collations, operator_class))
    }
}

impl InsertInferenceModifiers {
    pub(crate) fn collect_ast_collations<'ast>(ast: &'ast Expr, names: &mut Vec<&'ast Name>) {
        match ast {
            Expr::Collate(collate) => Self::collect_ast_collate_names(collate.as_ref(), names),
            Expr::Parenthesized(items) => Self::collect_ast_parenthesized_collations(items, names),
            _ => {}
        }
    }
}

impl InsertInferenceModifiers {
    fn collect_ast_parenthesized_collations<'ast>(
        items: &'ast [Expr],
        names: &mut Vec<&'ast Name>,
    ) {
        if let [expression] = items {
            Self::collect_ast_collations(expression, names);
        }
    }
}

impl InsertInferenceModifiers {
    fn collect_ast_collate_names<'ast>(ast: &'ast Collate, names: &mut Vec<&'ast Name>) {
        Self::collect_ast_collations(ast.argument(), names);
        names.push(ast.name());
    }
}

impl InsertClauseSemantics {
    pub(crate) fn ast_merge_override(ast: &OverridingKind) -> Option<String> {
        match ast {
            OverridingKind::NotSet => None,
            OverridingKind::UserValue => Some("user".to_owned()),
            OverridingKind::SystemValue => Some("system".to_owned()),
        }
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        insert: &Insert,
    ) -> Result<CteStatement, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            return AnalysisError::raise_unexpected_input("SQLite INSERT CTE");
        }
        let Some(mut insert) = Self::from_ast_in(insert, context, Some(scope))? else {
            return Ok(CteStatement::unresolved());
        };
        if columns.len() > insert.result_columns().len() {
            context.report_analysis_problem(AnalysisProblem::insert(
                InsertProblemKind::cte_aliases(),
                "INSERT CTE has more column aliases than RETURNING columns",
            ))?;
        }
        insert.rename_columns(columns);
        Ok(CteStatement::from_insert(insert))
    }
}

impl InsertStatement {
    pub(crate) fn from_ast_trigger_command(
        ast: &Insert,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemAst, ResolutionFailure> {
        DataSource::validate_ast_trigger_command(ast.relation())?;
        if !ast.returning().is_empty() || ast.source().is_none() {
            return SemCreateTrigger::ast_invalid(
                &FullName::from_ast_name(ast.relation().name())?,
                "SQLite trigger INSERT does not support RETURNING or DEFAULT VALUES",
            );
        }
        Self::from_ast_in(ast, context, Some(scope))
            .map(|statement| {
                statement
                    .as_ref()
                    .map_or_else(SemAst::unresolved_statement, SemAst::insert)
            })
            .map_err(Into::into)
    }
}
