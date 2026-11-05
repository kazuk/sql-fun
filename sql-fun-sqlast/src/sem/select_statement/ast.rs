use crate::{
    CacheContext,
    ast::{Dialect, Expr, Query, QueryBody, Select, SetOperation, SetQuery},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CteStatement, DataSource, FromClause, QueryGrouping,
        QueryLock, QueryOrder, QueryWindow, ResolutionFailure, ResultColumnCollection,
        RuleIdentity, ScalarAnalysisContext, SelectClauses, SelectDestination, SelectStatement,
        SelectStatementNode, SemScalarExpr, SemanticQuery, SetOperationKind, WithClause,
        problem::RuleProblem, query::ClauseBinding, with_stack,
    },
};

impl SelectStatement {
    #[cfg(test)]
    pub(crate) fn from_ast_test_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_in(ast, context, outer)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast(
        ast: &Query,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None);
        let result = context.infer_query_parameter_types(result, cache);
        context.finish_analysis(result)
    }
}

#[cfg(test)]
mod test_from_ast {
    // Semantic behavior at the query-analysis boundary. All fixtures use a small catalog.

    mod array_subquery {
        // ARRAY query semantics through the lightweight query-analysis boundary.

        use crate::{
            ast::Dialect,
            context::ParseContext,
            parse_statements,
            sem::{
                AnalysisError, AnalysisProblem, PgBuiltInType, SemAst, SemScalarExpr, TypeReference,
            },
            test_helpers::{
                create_cache_context,
                query::{catalog, expression as projection, resolve},
                query_array_subquery as test_helpers,
            },
        };

        use std::collections::BTreeMap;

        #[rstest::rstest]
        #[case::one("SELECT 1")]
        #[case::empty("SELECT 1 WHERE false")]
        #[case::many("SELECT 1 UNION ALL SELECT 2")]
        #[case::null("SELECT NULL::integer")]
        #[case::unknown_null("SELECT NULL")]
        #[case::unknown_string("SELECT 'hello'")]
        #[case::array("SELECT ARRAY[1,2]")]
        #[case::empty_array_result("SELECT NULL::integer[] WHERE false")]
        #[case::nested("SELECT ARRAY(SELECT 1)")]
        #[case::star("SELECT s.* FROM (SELECT 1 AS n) s")]
        #[case::clauses("SELECT 1 AS n ORDER BY n LIMIT 2 OFFSET 0")]
        fn array_query_retains_child_type_domain_identity(
            #[case] child: &str,
            #[values(false, true)] infer: bool,
        ) {
            let (query, context) = resolve(
                &format!("SELECT ARRAY({child}) AS result"),
                Dialect::postgres(),
                infer,
            )
            .unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let columns = query.result_columns();
            let column = columns.get_column_at(0).unwrap();
            let array = column.expression().unwrap().as_array_subquery().unwrap();
            assert_eq!(array.query().result_columns().len(), 1);
            assert_eq!(
                array.types().single_type().as_ref(),
                column.col_type().as_ref()
            );
        }

        #[rstest::rstest]
        #[case::zero("SELECT")]
        #[case::two("SELECT 1,2")]
        #[case::star("SELECT s.* FROM (SELECT 1 AS n, 2 AS m) s")]
        fn invalid_output_shape_retains_query_and_empty_domain(
            #[case] child: &str,
            #[values(false, true)] infer: bool,
        ) {
            let (query, _) = resolve(
                &format!("SELECT ARRAY({child})"),
                Dialect::postgres(),
                infer,
            )
            .unwrap();
            let array = projection(&query).as_array_subquery().unwrap();
            assert_ne!(array.query().result_columns().len(), 1);
            assert_eq!(array.types().admissible_types(), Some(&[][..]));
        }

        #[rstest::rstest]
        #[case::projection("SELECT ARRAY(SELECT $1::integer)")]
        #[case::predicate("SELECT ARRAY(SELECT 1 WHERE $1::integer IS NULL)")]
        #[case::order("SELECT ARRAY(SELECT 1 ORDER BY $1::integer)")]
        #[case::correlated(
            "SELECT ARRAY(SELECT s.n WHERE $1::integer IS NULL) FROM (SELECT 1 AS n) s"
        )]
        #[case::nested("SELECT ARRAY(SELECT ARRAY(SELECT $1::integer))")]
        #[case::repeated("SELECT ARRAY(SELECT $1::integer), $1::integer")]
        fn child_parameters_are_visited_and_finalized(#[case] sql: &str) {
            let (mut query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let constraints = context.parameter_constraints();
            assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
            assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
            let mut parameters = Parameters(Vec::new());
            query.walk(&mut parameters).unwrap();
            assert!(!parameters.0.is_empty());
            assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int2())])));
            // Both smallint and integer inputs can satisfy the cast, so the parameter
            // has no single inferred type even after its constraints are finalized.
            assert!(parameters.0.into_iter().all(|ty| ty.is_none()));
        }

        #[test]
        fn array_query_projection_preserves_both_source_shapes_and_correlations() {
            let (query, context) = resolve(
                "SELECT CASE WHEN true THEN ARRAY(SELECT $1) ELSE $2::integer[] END",
                Dialect::postgres(),
                true,
            )
            .unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let constraints = context.parameter_constraints();
            for input in [PgBuiltInType::int4(), PgBuiltInType::int4().array_type()] {
                assert!(constraints.allows(&BTreeMap::from([(1, input)])));
            }
            assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
            assert_eq!(
                projection(&query).get_type(),
                Some(PgBuiltInType::int4().array_type())
            );
        }

        #[test]
        fn unconstrained_source_keeps_functional_array_relationship() {
            let (query, context) =
                resolve("SELECT ARRAY(SELECT $1)", Dialect::postgres(), true).unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let constraints = context.parameter_constraints();
            assert!(constraints.parameter(1).unwrap().constraint().is_any());
            assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
            assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4().array_type())])));
            assert!(
                projection(&query)
                    .as_array_subquery()
                    .unwrap()
                    .types()
                    .is_any()
            );
        }

        #[test]
        fn anonymous_record_identity_is_retained_in_array_type() {
            let (query, context) =
                resolve("SELECT ARRAY(SELECT ROW(1,'x'))", Dialect::postgres(), true).unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let array = projection(&query).get_type().unwrap();
            assert_eq!(array.is_array(), Some(true));
            assert!(array.element_type().type_definition().is_some());
        }

        #[test]
        fn correlated_binding_and_serialized_query_survive_analysis() {
            let (query, context) = resolve(
        "SELECT ARRAY(SELECT s.n WHERE s.n IS NOT NULL ORDER BY s.n) FROM (SELECT 1 AS n) s",
        Dialect::postgres(),
        true,
    )
    .unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let array = projection(&query).as_array_subquery().unwrap();
            let child = projection(array.query());
            assert!(child.constraint_binding().is_some());
            assert_eq!(child.get_type(), Some(PgBuiltInType::int4()));
            let encoded = ::serde_json::to_string(projection(&query)).unwrap();
            let decoded: SemScalarExpr = ::serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded.get_type(), projection(&query).get_type());
            assert!(decoded.as_array_subquery().is_some());
        }

        struct Parameters(Vec<Option<TypeReference>>);
        impl crate::sem::traversal::SemanticVisitorMut for Parameters {
            fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if matches!(value, SemScalarExpr::Param(_)) {
                    self.0.push(value.get_type());
                }
                Ok(())
            }
        }

        #[test]
        fn synthetic_identity_function_consumes_array_subquery_result() {
            let (query, context) = resolve(
                "SELECT identity((ARRAY(SELECT 1))[1])",
                Dialect::postgres(),
                true,
            )
            .unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(projection(&query).get_type(), Some(PgBuiltInType::int4()));
        }

        #[test]
        fn invalid_array_query_does_not_stop_following_statement() {
            let mut cache_context = create_cache_context();
            let mut context = catalog();
            let mut results = Vec::new();
            for statement in parse_statements(
                "SELECT ARRAY(SELECT 1,2); SELECT ARRAY(SELECT 1)",
                Dialect::postgres(),
            ) {
                let analyzed =
                    SemAst::from_ast(&statement.unwrap(), context, &mut cache_context).unwrap();
                results.push(analyzed.ast().clone().into_select_statemtnt().unwrap());
                context = analyzed.into_context();
            }
            assert!(reports_width_problem(context.reported_problems()));
            assert_eq!(projection(&results[0]).get_type(), None);
            assert_eq!(
                projection(&results[1]).get_type(),
                Some(PgBuiltInType::int4().array_type())
            );
        }

        #[rstest::rstest]
        #[case::one("singleton", true)]
        #[case::two("pair", false)]
        fn deferred_star_checks_final_output_width(#[case] name: &str, #[case] valid: bool) {
            let analyzed = test_helpers::composite_query(&format!(
                "SELECT ARRAY(SELECT ($1).*) AS result, take_{name}($1)"
            ));
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let array = projection(&query).as_array_subquery().unwrap();
            if !valid {
                assert_eq!(array.types().admissible_types(), Some(&[][..]));
            }
            assert_eq!(
                array.query().result_columns().len(),
                if valid { 1 } else { 2 }
            );
        }

        fn reports_width_problem(problems: &[AnalysisProblem]) -> bool {
            for problem in problems {
                if problem.code() == "array_subquery_columns" {
                    return true;
                }
            }
            false
        }
    }
    mod call_arguments {
        // Supplied syntax and candidate-specific formal identities survive resolution.

        use crate::{
            context::ParseContext,
            names::{AliasName, FullName},
            sem::TypeReference,
            sem::{
                AnalysisError, PgBuiltInType, QueryNameResolution, ScalarConstExpr,
                SelectStatement, SemScalarExpr,
            },
            test_helpers::{
                call_arguments::{call, resolve},
                query::expression,
            },
        };

        #[rstest::rstest]
        #[case::reordered("SELECT f(c => 9, a => 1)", &["c", "a"], &[9, 1])]
        #[case::old_notation("SELECT f(c := 9, a := 1)", &["c", "a"], &[9, 1])]
        #[case::mixed("SELECT f(1, c => 9)", &["a", "c"], &[1, 9])]
        #[case::folded("SELECT f(A => 1)", &["a"], &[1])]
        fn binds_written_arguments_without_inserting_defaults(
            #[case] sql: &str,
            #[case] formals: &[&str],
            #[case] values: &[i32],
        ) {
            let (query, context) = resolve(sql, false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let call = call(&query);
            assert!(!call.signature_is_deferred());
            assert!(call.overload().is_some());
            assert_eq!(call.args().len(), formals.len());
            assert_eq!(
                call.name_resolution(),
                &QueryNameResolution::function(&FullName::with_schema("public", "f"))
            );
            for (position, formal) in formals.iter().enumerate() {
                let argument = &call.args()[position];
                assert_eq!(argument.position(), position);
                assert_eq!(
                    argument
                        .arg_definition()
                        .as_ref()
                        .unwrap()
                        .name()
                        .map(String::as_str),
                    Some(*formal)
                );
                assert_eq!(
                    argument.value(),
                    &SemScalarExpr::literal(&ScalarConstExpr::new_integer(values[position]))
                );
                if argument.supplied_name().is_some() {
                    assert!(argument.source_span().is_some());
                }
            }
            let original = ::serde_json::to_value(expression(&query)).unwrap();
            let restored: SemScalarExpr = ::serde_json::from_value(original.clone()).unwrap();
            assert_eq!(::serde_json::to_value(restored).unwrap(), original);
            // Omitted defaults do not prove non-nullness for a strict function.
            assert_eq!(expression(&query).is_not_null(), Some(false));
        }

        #[rstest::rstest]
        #[case::duplicate("SELECT f(a => 1, a => 2)", "call_argument_duplicate_name")]
        #[case::folded_duplicate("SELECT f(A => 1, a => 2)", "call_argument_duplicate_name")]
        #[case::placement("SELECT f(a => 1, 2)", "call_argument_positional_after_named")]
        #[case::unknown_name("SELECT f(z => 1)", "call_argument_unknown_name")]
        #[case::quoted_name(r#"SELECT f("A" => 1)"#, "call_argument_unknown_name")]
        #[case::assigned_twice("SELECT f(1, a => 2)", "call_argument_assigned_twice")]
        #[case::missing_required("SELECT f(c => 1)", "call_argument_missing_required")]
        #[case::wrong_type("SELECT f(a => TRUE)", "function_overload_resolution_failed")]
        #[case::missing_function("SELECT missing(a => 1)", "function_not_found")]
        fn failed_binding_retains_arguments_and_lookup(#[case] sql: &str, #[case] code: &str) {
            let (query, context) = resolve(sql, false);
            assert_eq!(
                context.reported_problem_count(),
                1,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(context.reported_problems()[0].code(), code);
            let call = call(&query);
            assert!(call.overload().is_none());
            assert!(!call.args().is_empty());
            assert_eq!(
                call.name_resolution().is_resolved(),
                code != "function_not_found"
            );
        }

        #[rstest::rstest]
        #[case::positional("SELECT v(VARIADIC ARRAY[1,2])", None)]
        #[case::named("SELECT v(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
        #[case::ordinary_array("SELECT arr(VARIADIC xs => ARRAY[1,2])", Some("xs"))]
        fn explicit_variadic_binds_an_array_identity(
            #[case] sql: &str,
            #[case] name: Option<&str>,
        ) {
            let (query, context) = resolve(sql, true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let call = call(&query);
            assert!(call.modifiers().variadic());
            assert!(call.overload().is_some());
            assert_eq!(call.args().len(), 1);
            assert_eq!(call.args()[0].supplied_name(), name);
            assert_eq!(
                call.args()[0].arg_definition().as_ref().unwrap().get_type(),
                Some(&PgBuiltInType::int4().array_type())
            );
        }

        #[test]
        fn inference_preserves_reordered_positions_and_formal_associations() {
            let (mut query, context) = resolve(
                "SELECT f(c => $7::integer, a => $2::integer, b => $7::integer)",
                true,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            for position in [7, 2] {
                assert_eq!(
                    context
                        .parameter_constraints()
                        .parameter(position)
                        .unwrap()
                        .constraint()
                        .admissible_types(),
                    Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
                );
            }
            let SemScalarExpr::Constrained(constrained) = expression(&query) else {
                panic!("constrained call")
            };
            let binding = constrained.function_binding().unwrap();
            assert!(binding.overload().is_some());
            assert_eq!(
                binding
                    .argument_definition(0)
                    .unwrap()
                    .name()
                    .map(String::as_str),
                Some("c")
            );
            struct Parameters(Vec<u32>);
            impl crate::sem::traversal::SemanticVisitorMut for Parameters {
                fn expression(
                    &mut self,
                    expression: &mut SemScalarExpr,
                ) -> Result<(), AnalysisError> {
                    if let SemScalarExpr::Param(value) = expression {
                        self.0.push(value.position());
                    }
                    Ok(())
                }
            }
            let mut parameters = Parameters(Vec::new());
            query.walk(&mut parameters).unwrap();
            assert_eq!(parameters.0, [7, 2, 7]);
            let serialized = ::serde_json::to_value(&query).unwrap();
            let restored: SelectStatement = ::serde_json::from_value(serialized.clone()).unwrap();
            assert_eq!(::serde_json::to_value(restored).unwrap(), serialized);
        }

        #[rstest::rstest]
        #[case::scalar("SELECT f(a => $1) FROM f(a => 1) AS t")]
        #[case::table("SELECT t.* FROM f(a => $1::integer) AS t")]
        #[case::rows_from("SELECT t.* FROM ROWS FROM(f(a => $1)) AS t")]
        fn table_consumers_receive_call_constraints(#[case] sql: &str) {
            let (query, context) = resolve(sql, true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(
                context
                    .parameter_constraints()
                    .parameter(1)
                    .unwrap()
                    .constraint()
                    .admissible_types(),
                Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
            );
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("t"))
                .unwrap()
                .0;
            if let Some(function) = source.deferred_function() {
                assert!(function.call().overload().is_some());
            }
        }

        #[rstest::rstest]
        #[case::scalar("SELECT poly(value => $1), f(a => $1)")]
        #[case::table("SELECT t.* FROM poly(value => $1) AS t, f(a => $1) AS other")]
        fn deferred_polymorphism_uses_other_occurrences(#[case] sql: &str) {
            let (query, context) = resolve(sql, true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(expression(&query).get_type(), None);
            assert_eq!(
                context
                    .parameter_constraints()
                    .result_column(0)
                    .unwrap()
                    .admissible_types(),
                Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
            );
        }

        #[rstest::rstest]
        #[case::aggregate("SELECT named_sum(x=>1)", false)]
        #[case::window("SELECT named_sum(x=>1) OVER ()", true)]
        fn aggregate_notation_has_window_specific_legality(#[case] sql: &str, #[case] valid: bool) {
            // An explicit catalog isolates call legality from CREATE AGGREGATE parsing.
            let (query, context) = resolve(sql, false);
            assert_eq!(context.reported_problem_count(), usize::from(!valid));
            assert_eq!(call(&query).overload().is_some(), valid);
            if !valid {
                assert_eq!(
                    context.reported_problems()[0].code(),
                    "function_overload_resolution_failed"
                );
            }
        }

        #[test]
        fn named_polymorphism_retains_nondefault_assignments_and_correlations() {
            let (query, context) = resolve("SELECT same(b => $7, a => $2)", true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert!(expression(&query).get_type().is_none());
            let constraints = context.parameter_constraints();
            let bytea = TypeReference::from_master_type_name("bytea");
            assert!(constraints.allows(&std::collections::BTreeMap::from([
                (7, bytea.clone()),
                (2, bytea.clone())
            ])));
            assert!(!constraints.allows(&std::collections::BTreeMap::from([
                (7, bytea),
                (2, PgBuiltInType::int4())
            ])));
        }

        #[test]
        fn deferred_polymorphism_uses_a_nondefault_type_from_a_later_occurrence() {
            let (query, context) = resolve("SELECT poly(value=>$1), bytes(value=>$1)", true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(
                expression(&query).get_type(),
                Some(TypeReference::from_master_type_name("bytea"))
            );
        }

        #[test]
        fn deferred_out_columns_receive_argument_types() {
            let (query, context) = resolve(
                "SELECT t.* FROM polyout(value=>$1) AS t, bytes(value=>$1) AS other",
                true,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(
                expression(&query).get_type(),
                Some(TypeReference::from_master_type_name("bytea"))
            );
        }

        #[test]
        fn deferred_variadic_array_keeps_element_and_array_identity_distinct() {
            let (query, context) = resolve("SELECT polyarray(VARIADIC xs=>$1), arr(xs=>$1)", true);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        }
    }
    mod column_scope {
        use crate::{
            ast::Dialect,
            sem::{AnalysisError, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
            test_helpers::query::column_scope_query,
        };

        #[rstest::rstest]
        #[case::original("SELECT (SELECT id FROM missing.items) FROM outer_items;")]
        #[case::competing_known(
            "SELECT (SELECT id FROM known_items, missing.items) FROM outer_items"
        )]
        #[case::unknown_first(
            "SELECT (SELECT id FROM missing.items, known_items) FROM outer_items"
        )]
        #[case::multiple_unknown(
            "SELECT (SELECT id FROM missing.items a, missing.other b) FROM outer_items"
        )]
        #[case::join(
            "SELECT (SELECT id FROM known_items CROSS JOIN missing.items) FROM outer_items"
        )]
        #[case::join_unknown_first(
            "SELECT (SELECT id FROM missing.items CROSS JOIN known_items) FROM outer_items"
        )]
        #[case::nested_unknown("SELECT (SELECT (SELECT id) FROM missing.items) FROM outer_items")]
        #[case::deep_unknown(
            "SELECT (SELECT (SELECT id FROM missing.items) FROM local_items) FROM outer_items"
        )]
        #[case::shadowed_alias(
            "SELECT (SELECT outer_items.id FROM missing.items outer_items) FROM outer_items"
        )]
        #[case::qualified_missing("SELECT (SELECT items.id FROM missing.items) FROM outer_items")]
        fn unresolved_local_relation_blocks_outer_binding(#[case] sql: &str) {
            let (mut query, context) = column_scope_query(sql, Dialect::postgres());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert!(bindings.tables.is_empty(), "{bindings:?}");
            assert_eq!(bindings.unresolved, 1, "{bindings:?}");
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "relation_not_found")
            );
        }

        #[rstest::rstest]
        #[case::missing("SELECT (SELECT id FROM missing()) FROM outer_items")]
        #[case::competing_known("SELECT (SELECT id FROM known_items, missing()) FROM outer_items")]
        #[case::unknown_first("SELECT (SELECT id FROM missing(), known_items) FROM outer_items")]
        #[case::join("SELECT (SELECT id FROM known_items CROSS JOIN missing()) FROM outer_items")]
        #[case::nested("SELECT (SELECT (SELECT id) FROM missing()) FROM outer_items")]
        fn unresolved_table_function_blocks_outer_binding(#[case] sql: &str) {
            let (mut query, context) = column_scope_query(sql, Dialect::postgres());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert!(bindings.tables.is_empty(), "{bindings:?}");
            assert_eq!(bindings.unresolved, 1, "{bindings:?}");
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "table_function_not_found")
            );
        }

        #[rstest::rstest]
        #[case::resolved_absence(
            "SELECT (SELECT id FROM local_items) FROM outer_items",
            "outer_items"
        )]
        #[case::empty_from("SELECT (SELECT id) FROM outer_items", "outer_items")]
        #[case::nested_absence(
            "SELECT (SELECT (SELECT id FROM local_items)) FROM outer_items",
            "outer_items"
        )]
        #[case::local_binding(
            "SELECT (SELECT id FROM known_items) FROM outer_items",
            "known_items"
        )]
        #[case::local_shadowing(
            "SELECT (SELECT outer_items.id FROM known_items outer_items) FROM outer_items",
            "known_items"
        )]
        fn conclusive_lookup_preserves_binding(#[case] sql: &str, #[case] relation: &str) {
            let (mut query, context) = column_scope_query(sql, Dialect::postgres());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert_eq!(bindings.tables, [relation]);
            assert_eq!(bindings.unresolved, 0);
            assert_eq!(context.reported_problem_count(), 0);
        }

        #[test]
        fn explicit_outer_qualification_bypasses_unrelated_unknown_source() {
            let (mut query, context) = column_scope_query(
                "SELECT (SELECT outer_items.id FROM missing.items) FROM outer_items",
                Dialect::postgres(),
            );
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert_eq!(bindings.tables, ["outer_items"]);
            assert_eq!(bindings.unresolved, 0);
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "relation_not_found")
            );
        }

        #[rstest::rstest]
        #[case::known("known_items a, known_items b")]
        #[case::unknown_first("missing.items, known_items a, known_items b")]
        #[case::unknown_last("known_items a, known_items b, missing.items")]
        #[case::join("known_items a CROSS JOIN known_items b CROSS JOIN missing.items")]
        fn established_ambiguity_survives_unknown_sources(#[case] from: &str) {
            let sql = format!("SELECT (SELECT id FROM {from}) FROM outer_items");
            let (mut query, context) = column_scope_query(&sql, Dialect::postgres());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert!(bindings.tables.is_empty());
            assert_eq!(bindings.ambiguous, 1);
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "ambiguous_column")
            );
        }

        #[test]
        fn conclusive_missing_column_remains_unresolved() {
            let (mut query, context) =
                column_scope_query("SELECT absent FROM local_items", Dialect::postgres());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert!(bindings.tables.is_empty());
            assert_eq!(bindings.unresolved, 1);
            assert_eq!(context.reported_problem_count(), 1);
        }

        #[test]
        fn unknown_input_does_not_bind_grouping_output_alias() {
            let (mut query, _) = column_scope_query(
                "SELECT 1 AS id FROM missing.items GROUP BY id",
                Dialect::postgres(),
            );
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert_eq!(bindings.unresolved, 1);
            assert_eq!(bindings.outputs, 0);
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::group("SELECT (SELECT id FROM (missing_items) AS g) FROM outer_items")]
        #[case::group_join(
            "SELECT (SELECT id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
        )]
        #[case::qualified_group(
            "SELECT (SELECT g.id FROM (known_items CROSS JOIN missing_items) AS g) FROM outer_items"
        )]
        fn sqlite_group_preserves_inconclusive_lookup(#[case] sql: &str) {
            let (mut query, context) = column_scope_query(sql, Dialect::sqlite());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert!(bindings.tables.is_empty());
            assert_eq!(bindings.unresolved, 1);
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "relation_not_found")
            );
        }

        #[derive(Default, Debug)]
        struct Bindings {
            tables: Vec<String>,
            unresolved: usize,
            ambiguous: usize,
            outputs: usize,
        }

        impl crate::sem::traversal::SemanticVisitorMut for Bindings {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if expression.output_index().is_some() {
                    self.outputs += 1;
                }
                if let SemScalarExpr::ColumnRef(reference) = expression {
                    if let Some(table) = reference.table_column_reference() {
                        self.tables
                            .push(table.relation().local_name().as_str().to_owned());
                        assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
                    } else if matches!(reference, ColumnReferenceExpr::AmbiguousColumnRef(_)) {
                        self.ambiguous += 1;
                        assert_eq!(expression.get_type(), None);
                    } else if matches!(
                        reference,
                        ColumnReferenceExpr::UndefinedColumn(_)
                            | ColumnReferenceExpr::UndefinedTableOrView(_)
                            | ColumnReferenceExpr::UndefinedAlias(_)
                            | ColumnReferenceExpr::UndefinedFunction(_)
                    ) {
                        self.unresolved += 1;
                        assert_eq!(expression.get_type(), None);
                    }
                }
                Ok(())
            }
        }
    }
    mod derived_aliases {
        // Derived-column visibility through the maintained SQL-to-semantics boundary.
        use crate::{
            ast::Dialect,
            sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
            test_helpers::query::resolve,
        };

        #[rstest::rstest]
        #[case::union(
            "SELECT renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)",
            "SELECT x.renamed FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)"
        )]
        #[case::simple(
            "SELECT renamed FROM (SELECT 1 AS n) x(renamed)",
            "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed)"
        )]
        #[case::partial(
            "SELECT b FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
            "SELECT x.b FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
        )]
        #[case::renamed_partial(
            "SELECT renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)",
            "SELECT x.renamed FROM (SELECT 1 AS a, 2 AS b) x(renamed)"
        )]
        #[case::hidden_name_does_not_compete(
            "SELECT n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y",
            "SELECT y.n FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y"
        )]
        #[case::local_shadowing(
            "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)",
            "SELECT (SELECT y.renamed FROM (SELECT 2 AS n) y(renamed)) FROM (SELECT 1 AS n) x(renamed)"
        )]
        #[case::correlated(
            "SELECT (SELECT renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)",
            "SELECT (SELECT x.renamed FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x(renamed)"
        )]
        #[case::hidden_local_name_allows_correlation(
            "SELECT (SELECT n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x",
            "SELECT (SELECT x.n FROM (SELECT 2 AS n) y(other)) FROM (SELECT 1 AS n) x"
        )]
        #[case::qualified_disambiguates(
            "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
            "SELECT x.renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)"
        )]
        fn visible_columns_retain_qualified_binding(#[case] sql: &str, #[case] qualified: &str) {
            let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let (expected, context) = resolve(qualified, Dialect::postgres(), false).unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let column = query.result_columns().get_column_at(0).unwrap();
            let expected_column = expected.result_columns().get_column_at(0).unwrap();
            // Equality includes the source alias, requested name and retained column definition.
            assert_eq!(column.expression(), expected_column.expression());
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            assert_eq!(
                column.expression().unwrap().is_not_null(),
                expected_column.expression().unwrap().is_not_null()
            );
            assert_eq!(column.column_name(), expected_column.column_name());
        }

        #[rstest::rstest]
        #[case::simple("SELECT n FROM (SELECT 1 AS n) x(renamed)")]
        #[case::union("SELECT n FROM (SELECT 1 AS n UNION ALL SELECT 2 AS n) x(renamed)")]
        fn hidden_original_name_is_unresolved(#[case] sql: &str) {
            let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
            assert_eq!(context.reported_problem_count(), 1);
            assert_eq!(
                context.reported_problems()[0].code(),
                "column_not_found_in_source"
            );
            assert_matches::assert_matches!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .expression(),
                Some(SemScalarExpr::ColumnRef(
                    ColumnReferenceExpr::UndefinedColumn(_)
                ))
            );
        }

        #[test]
        fn competing_derived_aliases_are_ambiguous() {
            let (query, context) = resolve(
                "SELECT renamed FROM (SELECT 1 AS n) x(renamed), (SELECT 2 AS n) y(renamed)",
                Dialect::postgres(),
                false,
            )
            .unwrap();
            assert_eq!(context.reported_problem_count(), 1);
            assert_eq!(context.reported_problems()[0].code(), "ambiguous_column");
            assert_matches::assert_matches!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .expression(),
                Some(SemScalarExpr::ColumnRef(
                    ColumnReferenceExpr::AmbiguousColumnRef(_)
                ))
            );
        }

        #[rstest::rstest]
        #[case::non_null("1", true)]
        #[case::nullable("NULL::int4", false)]
        fn renamed_column_retains_type_and_nullability(
            #[case] value: &str,
            #[case] not_null: bool,
        ) {
            let sql = format!("SELECT renamed FROM (SELECT {value} AS n) x(renamed)");
            let (query, context) = resolve(&sql, Dialect::postgres(), false).unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let column = query.result_columns().get_column_at(0).unwrap();
            assert_eq!(
                column.column_name(),
                &Some(crate::names::ColumnName::from("renamed"))
            );
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            assert_eq!(column.is_not_null(), &Some(not_null));
            assert_matches::assert_matches!(
                column.expression(),
                Some(SemScalarExpr::ColumnRef(
                    ColumnReferenceExpr::SubQueryColumn(_)
                ))
            );
        }
    }
    mod exists {
        // EXISTS behavior at the query-analysis boundary using a small catalog.

        use crate::{
            ast::Dialect,
            context::ParseContext,
            names::ColumnName,
            sem::{AnalysisError, PgBuiltInType, SemScalarExpr},
            test_helpers::query::{expression as projection, resolve},
        };

        #[rstest::rstest]
        #[case::null("SELECT EXISTS (SELECT NULL) AS found")]
        #[case::not_null("SELECT NOT EXISTS (SELECT NULL) AS found")]
        #[case::empty("SELECT EXISTS (SELECT NULL WHERE false) AS found")]
        #[case::not_empty("SELECT NOT EXISTS (SELECT NULL WHERE false) AS found")]
        #[case::integer("SELECT EXISTS (SELECT 42) AS found")]
        #[case::text("SELECT EXISTS (SELECT 'hello') AS found")]
        #[case::many_columns("SELECT EXISTS (SELECT 1, NULL) AS found")]
        #[case::no_columns("SELECT EXISTS (SELECT) AS found")]
        #[case::nested("SELECT EXISTS (SELECT EXISTS (SELECT NULL)) AS found")]
        #[case::boolean("SELECT NOT EXISTS (SELECT NULL) AND EXISTS (SELECT 1) AS found")]
        #[case::enclosing("SELECT found FROM (SELECT EXISTS (SELECT NULL) AS found) s")]
        fn exists_is_non_null_boolean(#[case] sql: &str, #[values(false, true)] infer: bool) {
            let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
            assert_eq!(context.reported_problem_count(), 0, "{sql}");
            let column = query
                .get_result_column_def(&ColumnName::from("found"))
                .unwrap();
            assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
            assert_eq!(column.is_not_null(), Some(true));
        }

        #[rstest::rstest]
        #[case::projection("SELECT EXISTS (SELECT $1::integer) AS found")]
        #[case::inferred_projection("SELECT EXISTS (SELECT $1 + 1) AS found")]
        #[case::predicate("SELECT EXISTS (SELECT NULL WHERE $1::integer < 2) AS found")]
        #[case::not("SELECT NOT EXISTS (SELECT $1::integer) AS found")]
        #[case::boolean("SELECT EXISTS (SELECT $1::integer) AND true AS found")]
        #[case::many_columns("SELECT EXISTS (SELECT $1::integer, NULL) AS found")]
        #[case::nested("SELECT EXISTS (SELECT EXISTS (SELECT $1::integer)) AS found")]
        #[case::enclosing("SELECT found FROM (SELECT EXISTS (SELECT $1::integer) AS found) s")]
        fn child_parameter_is_independent_of_boolean_result(#[case] sql: &str) {
            let (mut query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{sql}: {:?}",
                context.reported_problems()
            );
            let column = query
                .get_result_column_def(&ColumnName::from("found"))
                .unwrap();
            assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
            assert_eq!(column.is_not_null(), Some(true));
            assert_eq!(
                context
                    .parameter_constraints()
                    .parameter(1)
                    .unwrap()
                    .constraint()
                    .admissible_types(),
                Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
            );
            let mut parameters = Parameters::default();
            query.walk(&mut parameters).unwrap();
            assert_eq!(parameters.0, vec![1]);
        }

        #[derive(Default)]
        struct Parameters(Vec<u32>);

        impl crate::sem::traversal::SemanticVisitorMut for Parameters {
            fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Param(parameter) = value {
                    self.0.push(parameter.position());
                }
                Ok(())
            }
        }

        #[rstest::rstest]
        fn retains_correlated_query_and_serialized_predicate(#[values(false, true)] infer: bool) {
            let (query, context) = resolve(
                "SELECT EXISTS (SELECT 1 WHERE s.n IS NULL) AS found FROM (SELECT 1 AS n) s",
                Dialect::postgres(),
                infer,
            )
            .unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let expression = projection(&query);
            assert_eq!(expression.get_column_def(), None);
            assert!(expression.get_column_name().is_empty());
            let exists = expression.as_exists().expect("explicit EXISTS semantics");
            assert_eq!(exists.query().result_columns().len(), 1);
            let serialized = ::serde_json::to_string(expression).unwrap();
            let mut restored: SemScalarExpr = ::serde_json::from_str(&serialized).unwrap();
            assert!(restored.as_exists().is_some());
            assert_eq!(restored.get_type(), Some(PgBuiltInType::bool()));
            assert_eq!(restored.is_not_null(), Some(true));
            let mut bindings = Bindings::default();
            restored.walk(&mut bindings).unwrap();
            assert_eq!(bindings.correlated, 1);
            assert_eq!(bindings.missing, 0);
            assert_eq!(bindings.ambiguous, 0);
        }

        #[rstest::rstest]
        #[case::missing("SELECT EXISTS (SELECT missing) AS found", 1, 0)]
        #[case::ambiguous(
            "SELECT EXISTS (SELECT n FROM (SELECT 1 AS n) a, (SELECT 2 AS n) b) AS found",
            0,
            1
        )]
        fn retains_failed_child_bindings(
            #[case] sql: &str,
            #[case] missing: usize,
            #[case] ambiguous: usize,
        ) {
            let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
            assert!(context.reported_problem_count() > 0);
            assert!(projection(&query).as_exists().is_some());
            let mut bindings = Bindings::default();
            query.walk(&mut bindings).unwrap();
            assert_eq!(bindings.missing, missing);
            assert_eq!(bindings.ambiguous, ambiguous);
        }

        #[derive(Default)]
        struct Bindings {
            correlated: usize,
            missing: usize,
            ambiguous: usize,
        }

        impl crate::sem::traversal::SemanticVisitorMut for Bindings {
            fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                use crate::sem::ColumnReferenceExpr;
                match value {
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::SubQueryColumn(reference)) => {
                        assert_eq!(::serde_json::to_value(reference).unwrap()["alias"], "s");
                        self.correlated += 1;
                    }
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_)) => {
                        self.missing += 1
                    }
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_)) => {
                        self.ambiguous += 1
                    }
                    _ => {}
                }
                Ok(())
            }
        }

        #[test]
        fn scalar_subquery_remains_nullable_integer() {
            let (query, context) =
                resolve("SELECT (SELECT 1 AS n) AS value", Dialect::postgres(), true).unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let column = query
                .get_result_column_def(&ColumnName::from("value"))
                .unwrap();
            assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
            assert_eq!(column.is_not_null(), Some(false));
            assert!(projection(&query).as_exists().is_none());
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::null("SELECT EXISTS (SELECT NULL) AS found", 1)]
        #[case::not("SELECT NOT EXISTS (SELECT NULL) AS found", 0)]
        #[case::empty("SELECT EXISTS (SELECT NULL WHERE false) AS found", 0)]
        #[case::not_empty("SELECT NOT EXISTS (SELECT NULL WHERE false) AS found", 1)]
        #[case::many_columns("SELECT EXISTS (SELECT 1, NULL) AS found", 1)]
        fn sqlite_exists_uses_shared_semantics(#[case] sql: &str, #[case] expected: i32) {
            let (query, context) = resolve(sql, Dialect::sqlite(), true).unwrap();
            assert_eq!(context.reported_problem_count(), 0, "{sql}");
            let column = query
                .get_result_column_def(&ColumnName::from("found"))
                .unwrap();
            assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::bool()));
            assert_eq!(column.is_not_null(), Some(true));
            let connection = ::rusqlite::Connection::open_in_memory().unwrap();
            let result: i32 = connection.query_row(sql, [], |row| row.get(0)).unwrap();
            assert_eq!(result, expected);
        }
    }
    mod rows {
        // Row structure and field bindings use the small query catalog, without builtin loading.
        use crate::{
            ast::Dialect,
            context::ParseContext,
            names::ColumnName,
            sem::{PgBuiltInType, SemScalarExpr},
            test_helpers::query::resolve,
        };

        #[test]
        fn row_fields_preserve_order_types_nullability_and_nested_shape() {
            let (query, context) = resolve(
                "SELECT ROW(1, NULL, ROW(true, 2::bigint)) AS value",
                Dialect::postgres(),
                true,
            )
            .unwrap();
            assert_eq!(context.reported_problem_count(), 0);
            let value = query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap();
            let row = value.as_row().unwrap();
            assert_eq!(row.fields().len(), 3);
            assert_eq!(row.fields()[0].get_type(), Some(PgBuiltInType::int4()));
            assert_eq!(row.fields()[1].is_not_null(), Some(false));
            let nested = row.fields()[2].as_row().unwrap();
            assert_eq!(nested.fields()[0].get_type(), Some(PgBuiltInType::bool()));
            assert_eq!(nested.fields()[1].get_type(), Some(PgBuiltInType::int8()));
            assert!(value.get_type().unwrap().is_dynamic());
            let columns = value.composite_columns(&context).unwrap();
            assert_eq!(
                columns.column_at(0).unwrap().name(),
                &Some(ColumnName::from("f1"))
            );
            assert_eq!(columns.column_at(1).unwrap().is_not_null(), Some(false));
            let encoded = serde_json::to_string(value).unwrap();
            let decoded: SemScalarExpr = serde_json::from_str(&encoded).unwrap();
            assert_eq!(value, &decoded);
        }

        #[rstest::rstest]
        #[case::later("SELECT (ROW($1, 2)).f1 AS value, $1::integer AS hint", 0)]
        #[case::earlier("SELECT $1::integer AS hint, (ROW($1, 2)).f1 AS value", 1)]
        #[case::record_cast("SELECT (ROW($1,2)::record).f1 AS value, $1::integer AS hint", 0)]
        #[case::nested("SELECT ((ROW(2, ROW($1))).f2).f1 AS value, $1::integer AS hint", 0)]
        fn parameter_field_remains_connected_to_its_source(
            #[case] sql: &str,
            #[case] output: usize,
        ) {
            let (query, context) = resolve(sql, Dialect::postgres(), true).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(
                query
                    .get_result_column_def(&ColumnName::from("value"))
                    .unwrap()
                    .get_type_cloned(),
                None
            );
            assert_eq!(
                context
                    .parameter_constraints()
                    .result_column(output)
                    .unwrap()
                    .admissible_types(),
                Some([PgBuiltInType::int2(), PgBuiltInType::int4()].as_slice())
            );
            let expression = query
                .result_columns()
                .get_column_with_skip_count(&ColumnName::from("value"), 0)
                .unwrap()
                .expression()
                .unwrap();
            assert_eq!(expression.get_column_name(), "f1");
            assert!(expression.as_indirection().is_some());
            assert!(
                context
                    .parameter_constraints()
                    .allows(&std::collections::BTreeMap::from([(
                        1,
                        PgBuiltInType::int4()
                    )]))
            );
            assert!(
                !context
                    .parameter_constraints()
                    .allows(&std::collections::BTreeMap::from([(
                        1,
                        PgBuiltInType::bool()
                    )]))
            );
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::scalar("SELECT (1)", false)]
        #[case::row("SELECT (1,2)", true)]
        fn sqlite_projection_keeps_scalar_grouping_and_rejects_rows(
            #[case] sql: &str,
            #[case] invalid: bool,
        ) {
            let oracle = rusqlite::Connection::open_in_memory().unwrap();
            assert_eq!(oracle.prepare(sql).is_err(), invalid);
            let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
            let value = query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap();
            assert_eq!(value.as_row().is_some(), invalid);
            assert_eq!(context.reported_problem_count() > 0, invalid);
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::nested_row("SELECT ((1,2),3) IN (SELECT 1,3)")]
        #[case::cast("SELECT CAST((1,2) AS TEXT)")]
        #[case::nested_cast("SELECT CAST(CAST((1,2) AS TEXT) AS TEXT)")]
        #[case::unary_plus("SELECT +(1,2)")]
        #[case::null_test("SELECT (1,2) IS NULL")]
        #[case::not("SELECT NOT (1,2)")]
        #[case::and("SELECT (1,2) AND 1")]
        #[case::truth_test("SELECT (1,2) IS TRUE")]
        #[case::collate("SELECT (1,2) COLLATE binary")]
        #[case::parameter("SELECT CAST((?1,2) AS TEXT)")]
        #[case::coalesce("SELECT coalesce((1,2), 3)")]
        #[case::case_result("SELECT CASE WHEN 1 THEN (1,2) ELSE 3 END")]
        #[case::case_condition("SELECT CASE WHEN (1,2) THEN 1 ELSE 3 END")]
        #[case::case_argument("SELECT CASE (1,2) WHEN 1 THEN 2 END")]
        #[case::case_default("SELECT CASE WHEN 1 THEN 2 ELSE (1,2) END")]
        fn sqlite_scalar_consumers_reject_row_operands(#[case] sql: &str) {
            let oracle = rusqlite::Connection::open_in_memory().unwrap();
            assert!(oracle.prepare(sql).is_err(), "{sql}");
            let (_, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "query_scalar_value_required"),
                "{sql}: {:?}",
                context.reported_problems()
            );
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::membership("SELECT (1,2) IN (SELECT 1,2)")]
        #[case::is("SELECT (1,2) IS (1,2)")]
        fn sqlite_row_consumers_still_accept_row_operands(#[case] sql: &str) {
            let oracle = rusqlite::Connection::open_in_memory().unwrap();
            assert_eq!(
                oracle
                    .query_row(sql, [], |row| row.get::<_, i32>(0))
                    .unwrap(),
                1
            );
            let (_, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
        }

        #[test]
        fn every_row_field_retains_its_completed_binding() {
            let (query, context) = resolve(
                "SELECT ROW(t.x, public.absent.x, t.x) FROM (SELECT 1 AS x) t",
                Dialect::postgres(),
                false,
            )
            .unwrap();
            assert_eq!(context.reported_problem_count(), 1);
            let row = query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .as_row()
                .unwrap();
            assert_eq!(
                row.fields()[0].constraint_binding(),
                Some(("t", &ColumnName::from("x")))
            );
            assert_eq!(
                row.fields()[1].qualification_failure().unwrap().code(),
                "column_source_not_found"
            );
            assert_eq!(
                row.fields()[2].constraint_binding(),
                Some(("t", &ColumnName::from("x")))
            );
        }
    }
    mod source_namespace {
        #[cfg(feature = "sqlite")]
        use crate::{ast::Dialect, test_helpers::query::resolve};
        use crate::{
            names::FullName,
            sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr, TypeReference},
            test_helpers::{
                query::{source_namespace_statement, source_namespace_statement_at_version},
                query_source_namespace as test_helpers,
            },
        };

        #[rstest::rstest]
        #[case::comma("SELECT a.t.id, b.t.id FROM a.t, b.t")]
        #[case::reversed("SELECT a.t.id, b.t.id FROM b.t, a.t")]
        #[case::cross_join("SELECT a.t.id, b.t.id FROM a.t CROSS JOIN b.t")]
        #[case::inner_join("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON true")]
        #[case::search_path("SELECT a.t.id, b.t.id FROM t, b.t")]
        #[case::join_on("SELECT a.t.id, b.t.id FROM a.t JOIN b.t ON a.t.id IS NOT NULL")]
        fn distinct_relations_bind_without_problems(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            for (index, schema, ty) in [
                (0, "a", PgBuiltInType::int4()),
                (1, "b", PgBuiltInType::int8()),
            ] {
                let column = query.result_columns().get_column_at(index).unwrap();
                assert_eq!(
                    column
                        .expression()
                        .unwrap()
                        .table_column_reference()
                        .unwrap()
                        .relation(),
                    &FullName::with_schema(schema, "t")
                );
                assert_eq!(column.col_type(), &Some(ty));
            }
        }

        #[rstest::rstest]
        #[case::second("SELECT only_b FROM a.t, b.t", "b", "t", PgBuiltInType::int8())]
        #[case::first("SELECT only_b FROM b.t, a.t", "b", "t", PgBuiltInType::int8())]
        #[case::join(
            "SELECT only_b FROM a.t CROSS JOIN b.t",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::quoted(
            r#"SELECT "b.b"."t.x".id FROM "a.b"."t.x", "b.b"."t.x""#,
            "b.b",
            "t.x",
            PgBuiltInType::int8()
        )]
        #[case::outer(
            "SELECT (SELECT only_b FROM a.u) FROM a.t, b.t",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::join_alias(
            "SELECT j.only_b FROM (a.t CROSS JOIN b.t) AS j",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::join_alias_bare(
            "SELECT only_b FROM (a.t CROSS JOIN b.t) AS j",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::join_alias_renamed(
            "SELECT j.z FROM (a.t CROSS JOIN b.t) AS j(x,y,z,w)",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::hidden_namespace(
            "SELECT b.t.id FROM (a.t CROSS JOIN a.u) AS j, b.t",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        #[case::outer_qualifier(
            "SELECT (SELECT t.id FROM a.u) FROM b.t",
            "b",
            "t",
            PgBuiltInType::int8()
        )]
        fn unique_column_retains_its_occurrence(
            #[case] sql: &str,
            #[case] schema: &str,
            #[case] table: &str,
            #[case] ty: TypeReference,
        ) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let mut bindings = test_helpers::TableBindings::default();
            query.walk(&mut bindings).unwrap();
            bindings.assert_only_relation(&FullName::with_schema(schema, table));
            assert_eq!(
                query.result_columns().get_column_at(0).unwrap().col_type(),
                &Some(ty)
            );
        }

        #[rstest::rstest]
        #[case::common_column("SELECT t.id FROM a.t, b.t")]
        #[case::unique_column("SELECT t.only_b FROM a.t, b.t")]
        #[case::unique_other_order("SELECT t.only_b FROM b.t, a.t")]
        #[case::join("SELECT t.id FROM a.t CROSS JOIN b.t")]
        #[case::star("SELECT t.* FROM a.t, b.t")]
        #[case::local_blocks_outer("SELECT (SELECT t.id FROM a.t, b.t) FROM a.t AS t")]
        fn local_source_ambiguity_is_retained(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            let problems = analyzed.context().reported_problems();
            assert!(
                problems
                    .iter()
                    .any(|problem| problem.code() == "column_source_ambiguous"),
                "{problems:?}"
            );
            assert!(
                !problems
                    .iter()
                    .any(|problem| problem.code() == "duplicate_alias")
            );
            let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let mut failures = test_helpers::SourceFailures::default();
            query.walk(&mut failures).unwrap();
            failures.assert_source_ambiguity();
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .col_type()
                    .is_none()
            );
        }

        #[rstest::rstest]
        #[case::comma("SELECT id FROM a.t, b.t")]
        #[case::join("SELECT id FROM a.t CROSS JOIN b.t")]
        #[case::joined_alias("SELECT j.id FROM (a.t CROSS JOIN b.t) AS j")]
        fn bare_column_ambiguity_is_retained(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed
                    .context()
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "ambiguous_column")
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let column = query.result_columns().get_column_at(0).unwrap();
            assert!(matches!(
                column.expression(),
                Some(SemScalarExpr::ColumnRef(
                    ColumnReferenceExpr::AmbiguousColumnRef(_)
                ))
            ));
            assert!(column.col_type().is_none());
        }

        #[rstest::rstest]
        #[case::identical("SELECT 1 FROM a.t, a.t")]
        #[case::different_spelling("SELECT 1 FROM a.t, t")]
        #[case::explicit_same("SELECT 1 FROM a.t AS t, b.t")]
        #[case::explicit_right("SELECT 1 FROM a.t, b.t AS t")]
        #[case::both_explicit("SELECT 1 FROM a.t AS x, b.t AS x")]
        #[case::cte("WITH t AS (SELECT 1 AS id) SELECT 1 FROM t, a.t")]
        #[case::derived("SELECT 1 FROM (SELECT 1 AS id) AS t, b.t")]
        #[case::join("SELECT 1 FROM a.t CROSS JOIN a.t")]
        #[case::repeated_view("SELECT 1 FROM a.v, a.v")]
        #[case::update("UPDATE a.t SET id=1 FROM a.t")]
        #[case::delete("DELETE FROM a.t USING a.t")]
        #[case::merge("MERGE INTO a.t USING b.t ON true WHEN MATCHED THEN DELETE")]
        #[case::merge_ambiguous_source(
            "MERGE INTO a.t USING (b.t CROSS JOIN c.t) ON true WHEN MATCHED THEN DELETE"
        )]
        fn genuine_source_collisions_remain_invalid(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed
                    .context()
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "duplicate_alias"),
                "{:?}",
                analyzed.context().reported_problems()
            );
        }

        #[rstest::rstest]
        #[case::comma("SELECT * FROM a.t, b.t", &["a", "a", "b", "b"])]
        #[case::reversed("SELECT * FROM b.t, a.t", &["b", "b", "a", "a"])]
        #[case::schema_qualified("SELECT a.t.* FROM a.t, b.t", &["a", "a"])]
        #[case::join("SELECT * FROM a.t CROSS JOIN b.t", &["a", "a", "b", "b"])]
        #[case::left("SELECT * FROM a.t LEFT JOIN b.t ON true", &["a", "a", "b", "b"])]
        #[case::right("SELECT * FROM a.t RIGHT JOIN b.t ON true", &["a", "a", "b", "b"])]
        #[case::update("UPDATE a.t SET id=1 FROM b.t RETURNING *", &["a", "a"])]
        #[case::delete("DELETE FROM a.t USING b.t RETURNING *", &["a", "a"])]
        fn star_preserves_occurrences_and_output_order(
            #[case] sql: &str,
            #[case] schemas: &[&str],
        ) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let columns = test_helpers::result_columns(analyzed.ast());
            assert_eq!(columns.len(), schemas.len());
            for (index, schema) in schemas.iter().enumerate() {
                let column = columns.get_column_at(index).unwrap();
                assert_eq!(
                    column
                        .expression()
                        .unwrap()
                        .table_column_reference()
                        .unwrap()
                        .relation(),
                    &FullName::with_schema(schema, "t")
                );
                assert_eq!(
                    column.col_type(),
                    &Some(if *schema == "a" {
                        PgBuiltInType::int4()
                    } else {
                        PgBuiltInType::int8()
                    })
                );
                let null_extended = (sql.contains("LEFT JOIN") && *schema == "b")
                    || (sql.contains("RIGHT JOIN") && *schema == "a");
                assert_eq!(column.is_not_null(), &Some(!null_extended));
            }
        }

        #[rstest::rstest]
        #[case::views("SELECT a.v.id, b.v.id FROM a.v, b.v")]
        #[case::mixed("SELECT a.t.id, c.t.id FROM a.t, c.t")]
        fn views_share_the_relation_collision_rule(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            assert_eq!(
                query.result_columns().get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
            assert_eq!(
                query.result_columns().get_column_at(1).unwrap().col_type(),
                &Some(PgBuiltInType::int8())
            );
        }

        #[rstest::rstest]
        #[case::left("SELECT t.* FROM a.u LEFT JOIN b.t ON true", false)]
        #[case::right("SELECT t.* FROM b.t RIGHT JOIN a.u ON true", false)]
        #[case::inner("SELECT t.* FROM a.u CROSS JOIN b.t", true)]
        #[case::unique("SELECT only_b FROM a.t LEFT JOIN b.t ON true", false)]
        fn selected_occurrence_keeps_join_nullability(#[case] sql: &str, #[case] not_null: bool) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let column = query.result_columns().get_column_at(0).unwrap();
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
            assert_eq!(column.is_not_null(), &Some(not_null));
            assert_eq!(
                column
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema("b", "t")
            );
        }

        #[rstest::rstest]
        #[case::inner("SELECT * FROM a.t JOIN b.t USING(id)", &["a", "a", "b"])]
        #[case::right("SELECT * FROM a.t RIGHT JOIN b.t USING(id)", &["b", "a", "b"])]
        #[case::natural("SELECT * FROM a.t NATURAL JOIN b.t", &["a", "a", "b"])]
        #[case::aliased("SELECT j.* FROM (a.t CROSS JOIN b.t) j", &["a", "a", "b", "b"])]
        fn joined_star_keeps_selected_sources(#[case] sql: &str, #[case] schemas: &[&str]) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let columns = query.result_columns();
            assert_eq!(columns.len(), schemas.len());
            for (index, schema) in schemas.iter().enumerate() {
                assert_eq!(
                    columns
                        .get_column_at(index)
                        .unwrap()
                        .expression()
                        .unwrap()
                        .table_column_reference()
                        .unwrap()
                        .relation(),
                    &FullName::with_schema(schema, "t")
                );
            }
        }

        #[rstest::rstest]
        #[case::star("SELECT * FROM a.t FULL JOIN b.t USING(id)")]
        #[case::explicit("SELECT id, only_a, only_b FROM a.t FULL JOIN b.t USING(id)")]
        #[case::aliased("SELECT j.* FROM (a.t FULL JOIN b.t USING(id)) AS j")]
        fn full_join_using_keeps_merged_key_and_order(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let columns = query.result_columns();
            assert_eq!(
                columns
                    .names()
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>(),
                ["id", "only_a", "only_b"]
            );
            let merged = columns.get_column_at(0).unwrap();
            assert_eq!(merged.col_type(), &Some(PgBuiltInType::int8()));
            assert_eq!(merged.is_not_null(), &Some(true));
            let coalesce = merged.expression().unwrap().as_coalesce().unwrap();
            assert_eq!(coalesce.arguments().len(), 2);
            assert_eq!(
                coalesce.arguments()[0].get_type(),
                Some(PgBuiltInType::int8())
            );
            assert_eq!(
                coalesce.arguments()[1]
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema("b", "t")
            );
        }

        #[rstest::rstest]
        #[case::nested("SELECT * FROM a.t FULL JOIN b.t USING(id) FULL JOIN c.t USING(id)", &["id", "only_a", "only_b"])]
        #[case::natural("SELECT * FROM a.t NATURAL FULL JOIN b.t", &["id", "only_a", "only_b"])]
        fn full_join_merged_key_survives_other_join_shapes(
            #[case] sql: &str,
            #[case] names: &[&str],
        ) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let columns = query.result_columns();
            assert_eq!(
                columns
                    .names()
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>(),
                names
            );
            let key = columns.get_column_at(0).unwrap();
            assert_eq!(key.col_type(), &Some(PgBuiltInType::int8()));
            assert_eq!(key.is_not_null(), &Some(true));
            assert!(key.expression().unwrap().as_coalesce().is_some());
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::bare("SELECT * FROM (SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)")]
        #[case::grouped(
            "SELECT * FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j"
        )]
        fn sqlite_full_join_using_keeps_left_position_and_right_fallback(#[case] sql: &str) {
            let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
            let columns = query.result_columns();
            assert_eq!(columns.len(), 1);
            let key = columns.get_column_at(0).unwrap();
            assert!(key.expression().unwrap().as_coalesce().is_some());
            assert_eq!(key.column_name().as_ref().unwrap().as_str(), "id");
        }

        #[cfg(feature = "sqlite")]
        #[rstest::rstest]
        #[case::qualified("j.id")]
        #[case::unqualified("id")]
        fn sqlite_grouped_full_join_key_keeps_both_inputs(#[case] key: &str) {
            let (query, _) = resolve(
        &format!(
            "SELECT {key} FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j"
        ),
        Dialect::sqlite(),
        false,
    )
    .unwrap();
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .expression()
                    .unwrap()
                    .as_coalesce()
                    .is_some()
            );
        }

        #[rstest::rstest]
        #[case::qualified("SELECT (SELECT t.only_b FROM a.t) FROM b.t")]
        #[case::star("SELECT (SELECT t.only_b FROM a.t) FROM b.t AS t")]
        fn missing_local_column_does_not_bind_outer_source(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed
                    .context()
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "column_not_found_in_table")
            );
            let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let mut bindings = test_helpers::TableBindings::default();
            query.walk(&mut bindings).unwrap();
            bindings.assert_only_relation(&FullName::with_schema("a", "t"));
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .col_type()
                    .is_none()
            );
        }

        #[test]
        fn join_alias_hides_colliding_inner_occurrences() {
            let analyzed =
                source_namespace_statement("SELECT a.t.id FROM (a.t CROSS JOIN a.u) j, a.t");
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            assert_eq!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema("a", "t")
            );
        }

        #[test]
        fn locking_a_short_ambiguous_source_retains_failure() {
            let analyzed = source_namespace_statement("SELECT a.t.id FROM a.t,b.t FOR UPDATE OF t");
            assert!(
                analyzed
                    .context()
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "query_reference_ambiguous"),
                "{:?}",
                analyzed.context().reported_problems()
            );
        }

        #[rstest::rstest]
        #[case::table("SELECT j.z FROM (a.t CROSS JOIN b.t) j(x,y,z,w)")]
        #[case::view("SELECT j.z FROM (a.t CROSS JOIN b.v) j(x,y,z)")]
        #[case::cte("WITH c AS (SELECT id FROM b.t) SELECT j.z FROM (a.t CROSS JOIN c) j(x,y,z)")]
        #[case::derived("SELECT j.z FROM (a.t CROSS JOIN (SELECT id FROM b.t) s) j(x,y,z)")]
        fn joined_alias_keeps_its_exposed_column_name(#[case] sql: &str) {
            let analyzed = source_namespace_statement(sql);
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let column = query.result_columns().get_column_at(0).unwrap();
            assert_eq!(column.column_name().as_ref().unwrap().to_string(), "z");
            assert_eq!(column.expression().unwrap().get_column_name(), "z");
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
        }

        #[test]
        fn a_join_alias_preserves_occurrence_identity() {
            let analyzed = source_namespace_statement(
                "SELECT j.only_a, t.only_a FROM (a.t CROSS JOIN a.u) j, a.t",
            );
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            for (index, alias) in [(0, "j"), (1, "t")] {
                let binding = query
                    .result_columns()
                    .get_column_at(index)
                    .unwrap()
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap();
                assert_eq!(binding.alias().as_str(), alias);
            }
        }

        #[rstest::rstest]
        #[case::old("old", true)]
        #[case::new("new", false)]
        fn delete_row_images_keep_the_destination_occurrence(
            #[case] image: &str,
            #[case] not_null: bool,
        ) {
            let sql = format!("DELETE FROM a.t USING b.t RETURNING {image}.*");
            let analyzed = source_namespace_statement_at_version(&sql, "18");
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let columns = analyzed.ast().as_delete().unwrap().result_columns();
            assert_eq!(columns.len(), 2);
            for index in 0..columns.len() {
                let column = columns.get_column_at(index).unwrap();
                assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
                assert_eq!(column.is_not_null(), &Some(not_null));
                let expression = column.expression().unwrap();
                let SemScalarExpr::ColumnRef(reference) = expression else {
                    panic!("row-image column expected");
                };
                let row = reference.delete_row_reference().unwrap();
                assert_eq!(
                    row.source().table_column_reference().unwrap().relation(),
                    &FullName::with_schema("a", "t")
                );
            }
        }

        #[test]
        fn grouping_prefers_the_selected_source_over_an_output_alias() {
            let analyzed = source_namespace_statement(
                "SELECT 1 AS z FROM (a.t CROSS JOIN b.t) j(x,y,z,w) GROUP BY z",
            );
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            let mut bindings = test_helpers::TableBindings::default();
            query.walk(&mut bindings).unwrap();
            bindings.assert_only_relation(&FullName::with_schema("b", "t"));
        }

        #[test]
        fn merge_projection_keeps_source_role_and_join_alias() {
            let analyzed = source_namespace_statement(
                "MERGE INTO a.u AS d USING (a.t CROSS JOIN b.t) j(x,y,z,w) ON true WHEN MATCHED THEN DELETE RETURNING j.*",
            );
            assert!(
                analyzed.context().reported_problems().is_empty(),
                "{:?}",
                analyzed.context().reported_problems()
            );
            let columns = analyzed.ast().as_merge().unwrap().result_columns();
            assert_eq!(columns.len(), 4);
            for (index, name) in ["x", "y", "z", "w"].into_iter().enumerate() {
                let column = columns.get_column_at(index).unwrap();
                assert_eq!(column.column_name().as_ref().unwrap().to_string(), name);
                assert_eq!(column.expression().unwrap().get_column_name(), name);
                let record = column
                    .expression()
                    .unwrap()
                    .merge_record_reference()
                    .unwrap();
                assert_eq!(record.role(), crate::sem::MergeRecordRole::source());
                assert_eq!(
                    record
                        .source()
                        .table_column_reference()
                        .unwrap()
                        .alias()
                        .as_str(),
                    "j"
                );
            }
        }

        #[test]
        fn an_unresolved_join_input_blocks_qualified_binding() {
            let analyzed = source_namespace_statement(
                "SELECT (SELECT j.id FROM (a.t CROSS JOIN missing.t) j) FROM b.t",
            );
            assert!(
                analyzed
                    .context()
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "relation_not_found")
            );
            let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .col_type()
                    .is_none()
            );
        }
    }
    mod table_functions {
        use crate::{
            context::ParseContext,
            sem::{AnalysisError, ColumnReferenceExpr, PgBuiltInType, SemScalarExpr},
            test_helpers::query::{output_type, table_function_query},
        };

        #[test]
        fn typed_record_parameter_is_warning_free_in_the_controlled_context() {
            for infer in [false, true] {
                let (_, context) = table_function_query(
                    "SELECT a,b FROM record_fun($1::text) AS r(a int,b text)",
                    infer,
                );
                assert_eq!(
                    context.reported_problem_count(),
                    0,
                    "infer={infer}: {:?}",
                    context.reported_problems()
                );
            }
        }

        #[rstest::rstest]
        #[case::named("SELECT (pair_fun()).*")]
        #[case::table_row("SELECT (row_fun()).*")]
        #[case::column("SELECT x.* FROM (SELECT pair_fun() AS x) s")]
        #[case::qualified_column("SELECT (s.x).* FROM (SELECT pair_fun() AS x) s")]
        #[case::anonymous("SELECT (ROW(1::integer, 'x'::text)).*")]
        fn expands_composite_expression_targets(#[case] sql: &str) {
            let (query, context) = table_function_query(sql, true);
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
            let columns = query.result_columns();
            assert_eq!(
                columns.get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
            let expression = columns.get_column_at(0).unwrap().expression().unwrap();
            assert!(matches!(expression, SemScalarExpr::Indirection(_)));
        }

        #[test]
        fn unresolved_composite_star_retains_its_base_for_parameter_inference() {
            let (query, context) = table_function_query("SELECT ($1).*", true);
            let output = query.result_columns().get_column_at(0).unwrap();
            assert!(output.has_pending_star());
            assert!(output.column_name().is_none());
            assert!(output.expression().is_some());
            let serialized = ::serde_json::to_value(output).unwrap();
            assert!(serialized["star_expansion"]["Pending"].is_object());
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
        }

        #[test]
        fn inferred_composite_star_preserves_following_output_positions() {
            let (query, context) = table_function_query("SELECT ($1).*, $2, $1::public.pair", true);
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
            let columns = query.result_columns();
            assert_eq!(columns.len(), 4);
            assert_eq!(
                columns.get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
            assert_eq!(
                columns.get_column_at(1).unwrap().col_type(),
                &Some(PgBuiltInType::text())
            );
            let constraints = context.parameter_constraints();
            assert!(
                constraints
                    .result_column(0)
                    .unwrap()
                    .allows(&PgBuiltInType::int4())
            );
            assert!(
                constraints
                    .result_column(1)
                    .unwrap()
                    .allows(&PgBuiltInType::text())
            );
            assert!(
                constraints
                    .result_column(2)
                    .unwrap()
                    .allows(&PgBuiltInType::text())
            );
            assert!(constraints.result_column(3).is_some());
        }

        #[test]
        fn scalar_star_reports_a_structured_problem() {
            let (query, context) = table_function_query("SELECT (1).*", true);
            assert!(context.reported_problems().iter().any(|problem| {
                problem.code() == "query_star_projection_requires_composite_type"
            }));
            assert_eq!(query.result_columns().len(), 1);
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .column_name()
                    .is_none()
            );
        }

        #[test]
        fn row_star_in_scalar_argument_reports_placement() {
            let (_, context) = table_function_query("SELECT identity((ROW(1)).*)", true);
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| { problem.code() == "query_invalid_star_projection_placement" })
            );
        }

        #[test]
        fn anonymous_row_star_keeps_nullable_fields() {
            let (query, context) = table_function_query("SELECT (ROW(1::integer, NULL)).*", true);
            assert!(
                context.reported_problems().is_empty(),
                "{:?}",
                context.reported_problems()
            );
            let columns = query.result_columns();
            assert_eq!(columns.len(), 2);
            assert_eq!(columns.get_column_at(0).unwrap().is_not_null(), &Some(true));
            assert_eq!(
                columns.get_column_at(1).unwrap().is_not_null(),
                &Some(false)
            );
        }

        #[rstest::rstest]
        #[case::not_null("SELECT (ROW(1::integer)).*", Some(true))]
        #[case::nullable("SELECT (ROW(NULL)).*", Some(false))]
        fn composite_member_nullability_survives_serialization(
            #[case] sql: &str,
            #[case] expected: Option<bool>,
        ) {
            let (query, context) = table_function_query(sql, true);
            assert!(context.reported_problems().is_empty());
            let expression = query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap();
            assert_eq!(expression.is_not_null(), expected);
            let restored: SemScalarExpr =
                ::serde_json::from_value(::serde_json::to_value(expression).unwrap()).unwrap();
            assert_eq!(restored.is_not_null(), expected);
        }

        #[rstest::rstest]
        #[case::missing("SELECT 1 FROM record_fun('') r", "table_function_definition_required")]
        #[case::duplicate(
            "SELECT 1 FROM record_fun('') r(a int,a text)",
            "table_function_duplicate_column"
        )]
        #[case::unknown_type(
            "SELECT 1 FROM record_fun('') r(a missing)",
            "table_function_type_not_found"
        )]
        #[case::scalar(
            "SELECT 1 FROM identity(1) r(a int)",
            "table_function_definition_not_allowed"
        )]
        #[case::composite(
            "SELECT 1 FROM pair_fun() r(a int)",
            "table_function_definition_not_allowed"
        )]
        #[case::out(
            "SELECT 1 FROM out_fun() r(a int)",
            "table_function_definition_not_allowed"
        )]
        #[case::too_many_aliases(
            "SELECT 1 FROM pair_fun() r(a,b,c)",
            "table_function_too_many_aliases"
        )]
        #[case::unknown_collation(
            r#"SELECT 1 FROM record_fun('') r(a text COLLATE missing)"#,
            "collation_not_found"
        )]
        #[case::named_missing(
            "SELECT 1 FROM record_fun(src => '') r",
            "table_function_definition_required"
        )]
        #[case::rows_missing(
            "SELECT 1 FROM ROWS FROM(record_fun('')) r",
            "table_function_definition_required"
        )]
        #[case::rows_duplicate(
            "SELECT 1 FROM ROWS FROM(record_fun('') AS(a int,a text)) r",
            "table_function_duplicate_column"
        )]
        #[case::rows_multiple_outer(
            "SELECT 1 FROM ROWS FROM(record_fun(''),record_fun('')) r(a int)",
            "table_function_definition_placement"
        )]
        #[case::rows_conflicting(
            "SELECT 1 FROM ROWS FROM(record_fun('') AS(a int)) r(b int)",
            "table_function_definition_placement"
        )]
        #[case::rows_outer_ordinality(
            "SELECT 1 FROM ROWS FROM(record_fun('')) WITH ORDINALITY r(a int)",
            "table_function_definition_placement"
        )]
        #[case::ordinary_ordinality(
            "SELECT 1 FROM record_fun('') WITH ORDINALITY r(a int)",
            "table_function_definition_placement"
        )]
        fn diagnoses_invalid_definitions(#[case] sql: &str, #[case] code: &str) {
            let (_, context) = table_function_query(sql, false);
            assert!(
                context.has_reported_problem(code),
                "{sql}: {:?}",
                context.reported_problems()
            );
        }

        #[rstest::rstest]
        #[case::missing("SELECT r.missing FROM record_fun('') r(a int)", false)]
        #[case::rows_missing("SELECT r.missing FROM ROWS FROM(record_fun('') AS(a int)) r", false)]
        #[case::hidden_original("SELECT a FROM ROWS FROM(record_fun('') AS(a int)) r(x)", false)]
        #[case::rows_duplicate(
            "SELECT a FROM ROWS FROM(record_fun('') AS(a int),record_fun('') AS(a int)) r",
            true
        )]
        #[case::rows_qualified_duplicate(
            "SELECT r.a FROM ROWS FROM(record_fun('') AS(a int),record_fun('') AS(a int)) r",
            true
        )]
        fn retains_failed_or_ambiguous_binding(#[case] sql: &str, #[case] ambiguous: bool) {
            let (mut query, context) = table_function_query(sql, false);
            assert!(context.reported_problem_count() > 0);
            struct BindingFailure {
                ambiguous: bool,
                found: bool,
            }
            impl crate::sem::traversal::SemanticVisitorMut for BindingFailure {
                fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                    if let SemScalarExpr::ColumnRef(reference) = value {
                        if self.ambiguous {
                            self.found |=
                                matches!(reference, ColumnReferenceExpr::AmbiguousColumnRef(_));
                        } else {
                            self.found |= value.get_type().is_none();
                        }
                    }
                    Ok(())
                }
            }
            let mut failure = BindingFailure {
                ambiguous,
                found: false,
            };
            query.walk(&mut failure).unwrap();
            assert!(failure.found, "{query:?}");
        }

        #[test]
        fn composes_multiple_record_calls_in_written_order() {
            let (query, context) = table_function_query(
                "SELECT r.* FROM ROWS FROM(record_fun('') AS(a int),record_fun('') AS(b text)) r",
                false,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(
                query.result_columns().get_column_at(0).unwrap().col_type(),
                &Some(PgBuiltInType::int4())
            );
            assert_eq!(
                query.result_columns().get_column_at(1).unwrap().col_type(),
                &Some(PgBuiltInType::text())
            );
        }

        #[test]
        fn keeps_schema_qualified_and_array_field_types() {
            let (query, context) = table_function_query(
                "SELECT r.a, r.b FROM record_fun('') r(a pg_catalog.int4,b text[])",
                true,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(output_type(&query, "a"), Some(PgBuiltInType::int4()));
            assert_eq!(
                output_type(&query, "b"),
                Some(PgBuiltInType::text().array_type())
            );
        }

        #[test]
        fn keeps_selected_overload_separate_from_output_definitions() {
            use crate::{
                context::BaseParseContext,
                names::{AliasName, ColumnName, FullName},
                sem::DataSource,
            };
            let (query, context) =
                table_function_query("SELECT r.a FROM overloaded_record(1) AS r(a text)", false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("r"))
                .unwrap()
                .0;
            let DataSource::CallTableFunc(function) = source else {
                panic!("resolved function");
            };
            assert_eq!(
                function.function().get_arg_def_at(0).unwrap().get_type(),
                Some(&PgBuiltInType::int4())
            );
            assert_eq!(
                function
                    .output_columns()
                    .get_column_def(&ColumnName::from("a"))
                    .unwrap()
                    .get_type_cloned(),
                Some(PgBuiltInType::text())
            );
            let catalog = context
                .get_function_by_name(&FullName::from_local_name("overloaded_record"))
                .unwrap();
            assert_eq!(catalog.len(), 2);
            assert_eq!(catalog.record_definition_requirement(), Some(true));
            let restored: DataSource =
                ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
            assert_eq!(&restored, source);
        }

        #[rstest::rstest]
        #[case::qualified(
            "SELECT r.n FROM ROWS FROM(record_fun('') AS(a int)) WITH ORDINALITY r(a,n)",
            Some(true)
        )]
        #[case::star(
            "SELECT r.* FROM ROWS FROM(record_fun('') AS(a int)) WITH ORDINALITY r(a,n)",
            Some(true)
        )]
        #[case::nullable(
            "SELECT r.n FROM (SELECT 1 AS id) s LEFT JOIN ROWS FROM(record_fun('') AS(a int)) WITH ORDINALITY r(a,n) ON true",
            Some(false)
        )]
        fn preserves_ordinality_and_join_nullability(
            #[case] sql: &str,
            #[case] not_null: Option<bool>,
        ) {
            let (query, context) = table_function_query(sql, false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::int8()));
            assert_eq!(
                query
                    .get_result_column_def(&crate::names::ColumnName::from("n"))
                    .unwrap()
                    .is_not_null(),
                not_null
            );
        }

        #[rstest::rstest]
        #[case::missing("SELECT 1 FROM record_fun('') r")]
        #[case::duplicate("SELECT 1 FROM record_fun('') r(a int,a text)")]
        #[case::unknown_type("SELECT 1 FROM record_fun('') r(a missing)")]
        #[case::deferred_missing("SELECT 1 FROM record_fun(src => '') r")]
        fn retains_invalid_definition_state_after_serialization(#[case] sql: &str) {
            use crate::{names::AliasName, sem::DataSource};
            let (query, context) = table_function_query(sql, false);
            assert!(context.reported_problem_count() > 0);
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("r"))
                .unwrap()
                .0;
            let restored: DataSource =
                ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
            assert_eq!(&restored, source);
            let problems = if let Some(deferred) = restored.deferred_function() {
                deferred.output_problems()
            } else if let DataSource::CallTableFunc(function) = &restored {
                function.output_problems()
            } else {
                panic!("function invocation");
            };
            assert!(!problems.is_empty());
        }

        #[rstest::rstest]
        #[case::comma("SELECT s.* FROM (SELECT 1 AS id) s, ROWS FROM(record_fun('') AS(a int)) r")]
        #[case::join(
            "SELECT s.* FROM (SELECT 1 AS id) s CROSS JOIN ROWS FROM(record_fun('') AS(a int)) r"
        )]
        fn qualified_star_expands_only_its_selected_source(#[case] sql: &str) {
            let (query, context) = table_function_query(sql, false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(output_type(&query, "id"), Some(PgBuiltInType::int4()));
            assert!(query.result_columns().get_column_at(1).is_none());
        }

        #[test]
        fn table_row_returns_do_not_inherit_storage_constraints() {
            use crate::{
                context::BaseParseContext,
                names::{ColumnName, FullName, TableName},
            };
            let (query, context) = table_function_query("SELECT r.a FROM row_fun() r", false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(output_type(&query, "a"), Some(PgBuiltInType::int4()));
            let column = ColumnName::from("a");
            assert_eq!(
                query.get_result_column_def(&column).unwrap().is_not_null(),
                None
            );
            let table = context
                .get_table(&TableName::from(FullName::with_schema(
                    "public", "row_item",
                )))
                .unwrap();
            assert_eq!(
                table.get_column_def(&column).unwrap().is_not_null(),
                Some(true)
            );
        }

        #[rstest::rstest]
        #[case::varchar("varchar(3)", 7)]
        #[case::character("character(3)", 7)]
        #[case::array("varchar(3)[]", 7)]
        #[case::qualified("pg_catalog.varchar(3)", 7)]
        #[case::numeric("numeric(12,3)", (12 << 16) + 3 + 4)]
        #[case::negative_scale("numeric(2,-3)", (2 << 16) + 2045 + 4)]
        #[case::larger_scale("numeric(2,4)", (2 << 16) + 4 + 4)]
        #[case::string_precision("numeric('3')", (3 << 16) + 4)]
        #[case::bit("bit(5)", 5)]
        #[case::varbit("bit varying(5)", 5)]
        #[case::timestamp("timestamp(3)", 3)]
        #[case::timestamp_clamping("timestamp(9)", 6)]
        #[case::timetz("time(2) with time zone", 2)]
        #[case::interval("interval day to second(3)", (7176 << 16) + 3)]
        #[case::interval_clamping("interval(9)", (32767 << 16) + 6)]
        #[case::interval_range("interval year to month", (6 << 16) + 65535)]
        #[case::quoted_interval("\"interval\"(32767,3)", (32767 << 16) + 3)]
        fn retains_and_interprets_record_typmods(
            #[case] declared: &str,
            #[case] expected: i32,
            #[values(false, true)] rows: bool,
        ) {
            use crate::{names::AliasName, sem::DataSource};
            let sql = if rows {
                format!("SELECT r.* FROM ROWS FROM(record_fun('') AS(a {declared})) r")
            } else {
                format!("SELECT r.a FROM record_fun('') r(a {declared})")
            };
            let (mut query, context) = table_function_query(&sql, false);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("r"))
                .unwrap()
                .0;
            let restored: DataSource =
                ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
            assert_eq!(&restored, source);
            struct Modifiers {
                expected: i32,
                found: bool,
            }
            impl crate::sem::traversal::SemanticVisitorMut for Modifiers {
                fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                    if let SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableFuncColumn(column)) =
                        value
                    {
                        let declaration = column.type_declaration().unwrap();
                        assert_eq!(declaration.typmod(), Some(self.expected));
                        assert!(declaration.is_validated());
                        assert!(!declaration.arguments().is_empty());
                        self.found = true;
                    }
                    Ok(())
                }
            }
            let mut modifiers = Modifiers {
                expected,
                found: false,
            };
            query.walk(&mut modifiers).unwrap();
            assert!(modifiers.found);
        }

        #[rstest::rstest]
        #[case::zero_numeric("numeric(0)")]
        #[case::quoted_noninteger("numeric('''3''')")]
        #[case::large_numeric("numeric(1001)")]
        #[case::bad_scale("numeric(3,-1001)")]
        #[case::zero_varchar("varchar(0)")]
        #[case::large_varchar("varchar(10485761)")]
        #[case::negative_timestamp("\"timestamp\"(-1)")]
        #[case::zero_bit("bit varying(0)")]
        #[case::no_modifier("pg_catalog.int4(3)")]
        #[case::bad_interval_range("\"interval\"(1000)")]
        fn diagnoses_invalid_record_typmods(#[case] declared: &str) {
            let sql = format!("SELECT r.a FROM record_fun('') r(a {declared})");
            let (_, context) = table_function_query(&sql, false);
            assert!(
                context.has_reported_problem("table_function_invalid_typmod"),
                "{:?}",
                context.reported_problems()
            );
        }

        #[test]
        fn retains_extension_typmods_without_applying_builtin_rules() {
            use crate::{
                names::{AliasName, ColumnName},
                sem::DataSource,
            };
            let (query, context) = table_function_query(
                "SELECT r.a FROM record_fun(src => '') r(a public.custom_type(Point,4326))",
                false,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("r"))
                .unwrap()
                .0;
            let DataSource::DeferredTableFunc(source) = source else {
                panic!("deferred invocation")
            };
            let declaration = source
                .output_type_declaration(&ColumnName::from("a"))
                .unwrap();
            assert_eq!(declaration.arguments().len(), 2);
            assert!(!declaration.is_validated());
            assert!(declaration.validation_error().is_none());
        }

        #[test]
        fn preserves_distinct_rows_function_names_with_equal_signatures() {
            use crate::{
                names::{AliasName, FullName},
                sem::{DataSource, QueryNameResolution},
            };
            let (query, context) = table_function_query(
                "SELECT r.* FROM ROWS FROM(record_fun('') AS(a int),overloaded_record('') AS(b int)) r",
                false,
            );
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let source = query
                .query_clauses()
                .unwrap()
                .body()
                .source_scope()
                .unwrap()
                .resolve_alias(&AliasName::from("r"))
                .unwrap()
                .0;
            let restored: DataSource =
                ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
            let DataSource::RowFunctions(rows) = &restored else {
                panic!("ROWS FROM")
            };
            assert_eq!(
                rows.resolved_function_names(),
                vec![
                    crate::sem::QueryName::new(
                        "record_fun",
                        QueryNameResolution::function(&FullName::with_schema(
                            "public",
                            "record_fun"
                        ))
                    ),
                    crate::sem::QueryName::new(
                        "overloaded_record",
                        QueryNameResolution::function(&FullName::with_schema(
                            "public",
                            "overloaded_record"
                        ))
                    ),
                ]
            );
        }
    }
    mod whole_rows {
        // Whole-row arguments retain source identity and nominal/structural row types.
        use crate::{
            names::FullName,
            sem::{PgBuiltInType, TypeReference},
            test_helpers::{
                query::expression, query_whole_rows as test_helpers,
                query_whole_rows::catalog_query,
            },
        };

        #[rstest::rstest]
        #[case::positional(
            "SELECT accept_rows(a.*, t.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t"
        )]
        #[case::parenthesized(
            "SELECT accept_rows((a.*), (t.*)) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t"
        )]
        fn resolves_nominal_arguments(#[case] sql: &str) {
            let (query, context) = catalog_query(sql);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let call = expression(&query).as_function_call().unwrap();
            assert!(call.overload().is_some());
            assert_eq!(
                expression(&query).get_type(),
                Some(TypeReference::from_master_type_name("oid"))
            );
            for (index, (alias, relation)) in [("a", "pg_attribute"), ("t", "pg_type")]
                .into_iter()
                .enumerate()
            {
                let argument = &call.args()[index];
                let row = argument.value().as_whole_row().unwrap();
                assert_eq!(row.alias().unwrap().as_str(), alias);
                assert_eq!(
                    row.relation(),
                    Some(&FullName::with_schema("pg_catalog", relation))
                );
                assert_eq!(row.levels_up(), Some(0));
                assert!(row.failure_code().is_none());
                assert_eq!(
                    argument.value().get_type(),
                    Some(TypeReference::concrete_type_ref(
                        &FullName::with_schema("pg_catalog", relation),
                        false
                    ))
                );
                assert!(argument.arg_definition().is_some());
            }
        }

        #[test]
        fn named_arguments_retain_typed_rows() {
            let (query, context) = catalog_query(
                "SELECT accept_rows(left_row => a.*, right_row => t.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t",
            );
            assert_eq!(context.reported_problem_count(), 0);
            let call = expression(&query).as_function_call().unwrap();
            let row = call.args()[0].value().as_whole_row().unwrap();
            assert_eq!(call.args()[0].supplied_name(), Some("left_row"));
            assert_eq!(row.alias().unwrap().as_str(), "a");
            assert_eq!(
                call.args()[0].value().get_type(),
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "pg_attribute"),
                    false
                ))
            );
        }

        #[test]
        fn wrong_nominal_types_do_not_select_the_signature() {
            let (query, context) = catalog_query(
                "SELECT accept_rows(t.*, a.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t",
            );
            assert!(
                expression(&query)
                    .as_function_call()
                    .unwrap()
                    .overload()
                    .is_none()
            );
            test_helpers::assert_problem(&context, "function_overload_resolution_failed");
            assert_eq!(
                expression(&query).as_function_call().unwrap().args()[0]
                    .value()
                    .as_whole_row()
                    .unwrap()
                    .alias()
                    .unwrap()
                    .as_str(),
                "t"
            );
        }

        #[rstest::rstest]
        #[case::missing(
            "SELECT identity(missing.*) FROM pg_catalog.pg_type t",
            "column_source_not_found"
        )]
        #[case::missing_relation("SELECT identity(x.*) FROM missing x", "column_source_not_found")]
        #[case::hidden(
            "SELECT identity(pg_catalog.pg_type.*) FROM pg_catalog.pg_type t",
            "column_source_not_found"
        )]
        #[case::ambiguous(
            "SELECT identity(t.*) FROM pg_catalog.pg_type t CROSS JOIN pg_catalog.pg_attribute t",
            "column_source_ambiguous"
        )]
        fn retains_failed_lookup(#[case] sql: &str, #[case] code: &str) {
            let (query, context) = catalog_query(sql);
            let row = expression(&query).as_function_call().unwrap().args()[0]
                .value()
                .as_whole_row()
                .unwrap();
            assert_eq!(row.failure_code(), Some(code));
            assert!(row.alias().is_none());
            assert!(
                expression(&query).as_function_call().unwrap().args()[0]
                    .value()
                    .get_type()
                    .is_none()
            );
            test_helpers::assert_problem(&context, code);
        }

        #[rstest::rstest]
        #[case::aliased("SELECT type_arg(t.*) FROM pg_catalog.pg_type t", false)]
        #[case::qualified("SELECT type_arg(pg_catalog.pg_type.*) FROM pg_catalog.pg_type", false)]
        #[case::outer(
            "SELECT type_arg(t.*) FROM pg_catalog.pg_attribute a LEFT JOIN pg_catalog.pg_type t ON false",
            true
        )]
        fn retains_relation_and_source_nullability(#[case] sql: &str, #[case] nullable: bool) {
            let (query, context) = catalog_query(sql);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let value = expression(&query).as_function_call().unwrap().args()[0].value();
            assert_eq!(
                value.get_type(),
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", "pg_type"),
                    false
                ))
            );
            assert_eq!(value.is_not_null(), Some(!nullable));
        }

        #[rstest::rstest]
        #[case::derived("SELECT record_arg(x.*) FROM (SELECT 1 AS n) x")]
        #[case::cte("WITH x AS (SELECT 1 AS n) SELECT record_arg(x.*) FROM x")]
        #[case::nested("SELECT identity(record_arg(x.*)) FROM (SELECT 1 AS n) x")]
        fn derived_rows_bind_record_arguments(#[case] sql: &str) {
            let (query, context) = catalog_query(sql);
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        }

        #[test]
        fn correlation_records_the_selected_query_level() {
            let (query, context) =
                catalog_query("SELECT (SELECT record_arg(x.*)) FROM (SELECT 1 AS n) x");
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(test_helpers::row_levels(query), vec![1]);
        }

        #[test]
        fn local_source_shadows_an_outer_source_with_the_same_alias() {
            let (query, context) = catalog_query(
                "SELECT (SELECT record_arg(x.*) FROM (SELECT 2 AS n) x) FROM (SELECT 1 AS n) x",
            );
            assert_eq!(context.reported_problem_count(), 0);
            assert_eq!(test_helpers::row_levels(query), vec![0]);
        }

        #[test]
        fn derived_whole_row_retains_field_names_and_parameter_types() {
            let (query, context) = catalog_query("SELECT (x.*).n FROM (SELECT $1::integer AS n) x");
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            assert_eq!(test_helpers::row_levels(query), vec![0]);
        }

        #[rstest::rstest]
        #[case::member("SELECT (v.*).n FROM v")]
        #[case::star("SELECT (v.*).* FROM v")]
        fn view_whole_row_fields_remain_accessible(#[case] sql: &str) {
            let (query, context) = test_helpers::view_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            assert!(query.result_columns().get_column_at(1).is_none());
        }

        #[test]
        fn view_whole_row_rejects_a_missing_field() {
            let (query, context) = test_helpers::view_query("SELECT (v.*).missing FROM v");
            assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
            assert_eq!(
                context.problems()[0].code(),
                "composite_type_field_not_found"
            );
            assert!(expression(&query).get_type().is_none());
        }

        #[test]
        fn projection_star_keeps_its_column_expansion() {
            let (query, context) = catalog_query("SELECT t.* FROM pg_catalog.pg_type t");
            assert_eq!(context.reported_problem_count(), 0);
            assert!(expression(&query).table_column_reference().is_some());
            assert!(expression(&query).as_whole_row().is_none());
        }

        #[test]
        fn aggregate_star_does_not_become_a_whole_row_argument() {
            let (query, context) = catalog_query("SELECT count(*) FROM pg_catalog.pg_type t");
            assert_eq!(context.reported_problem_count(), 0);
            assert_eq!(
                expression(&query).as_function_call().unwrap().args().len(),
                0
            );
        }

        #[test]
        fn a_bare_call_star_does_not_select_an_implicit_row() {
            let (query, context) = catalog_query("SELECT identity(*) FROM pg_catalog.pg_type t");
            test_helpers::assert_problem(&context, "function_overload_resolution_failed");
            assert_eq!(
                expression(&query).as_function_call().unwrap().args().len(),
                0
            );
        }

        #[test]
        fn scalar_function_source_keeps_its_scalar_type() {
            let (query, context) = catalog_query("SELECT identity(x.*) FROM identity(1) x");
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{:?}",
                context.reported_problems()
            );
            let call = expression(&query).as_function_call().unwrap();
            assert!(call.overload().is_some());
            assert_eq!(
                call.args()[0].value().get_type(),
                Some(PgBuiltInType::int4())
            );
            let (column_query, _) = catalog_query("SELECT x FROM identity(1) x");
            assert_eq!(
                call.args()[0].value().is_not_null(),
                expression(&column_query).is_not_null()
            );
        }

        #[rstest::rstest]
        #[case::view("SELECT (r.*).x FROM v AS r(x)")]
        #[case::table("SELECT (r.*).x FROM t AS r(x)")]
        #[case::derived("SELECT (r.*).x FROM (SELECT 1 AS n) AS r(x)")]
        #[case::quoted(r#"SELECT (r.*)."X" FROM v AS r("X")"#)]
        #[case::function("SELECT (identity_v(r.*)).n FROM v AS r(x)")]
        #[case::nested_member("SELECT ((r.*).x).n FROM nested AS r(x)")]
        fn occurrence_aliases_expose_visible_fields(#[case] sql: &str) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            assert!(query.result_columns().get_column_at(1).is_none());
            assert!(expression(&query).as_indirection().is_some());
        }

        #[rstest::rstest]
        #[case::view("SELECT (r.*).* FROM v AS r(x)", "x")]
        #[case::table("SELECT (r.*).* FROM t AS r(x)", "x")]
        #[case::derived("SELECT (r.*).* FROM (SELECT 1 AS n) AS r(x)", "x")]
        #[case::function("SELECT (identity_v(r.*)).* FROM v AS r(x)", "n")]
        #[case::nested("SELECT ((r.*).x).* FROM nested AS r(x)", "n")]
        #[case::quoted(r#"SELECT (r.*).* FROM v AS r("X")"#, "X")]
        fn occurrence_star_preserves_visible_names(#[case] sql: &str, #[case] name: &str) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            let column = query.result_columns().get_column_at(0).unwrap();
            assert_eq!(column.column_name().as_ref().unwrap().as_str(), name);
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            assert!(query.result_columns().get_column_at(1).is_none());
            assert!(expression(&query).as_indirection().is_some());
        }

        #[rstest::rstest]
        #[case::view_original("SELECT (r.*).n FROM v AS r(x)")]
        #[case::view_missing("SELECT (r.*).missing FROM v AS r(x)")]
        #[case::table_original("SELECT (r.*).n FROM t AS r(x)")]
        #[case::table_missing("SELECT (r.*).missing FROM t AS r(x)")]
        #[case::derived_original("SELECT (r.*).n FROM (SELECT 1 AS n) AS r(x)")]
        #[case::derived_missing("SELECT (r.*).missing FROM (SELECT 1 AS n) AS r(x)")]
        #[case::nested_alias("SELECT ((r.*).x).x FROM nested AS r(x)")]
        #[case::function_alias("SELECT (identity_v(r.*)).x FROM v AS r(x)")]
        #[case::quoted_case(r#"SELECT (r.*).x FROM v AS r("X")"#)]
        fn occurrence_aliases_reject_hidden_and_missing_fields(#[case] sql: &str) {
            let (query, context) = test_helpers::alias_query(sql);
            assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
            assert_eq!(
                context.problems()[0].code(),
                "composite_type_field_not_found"
            );
            assert!(expression(&query).as_indirection().is_some());
            assert!(expression(&query).get_type().is_none());
        }

        #[rstest::rstest]
        #[case::members("SELECT (a.*).x, (b.*).y, (v.*).n FROM v AS a(x), v AS b(y), v")]
        #[case::stars("SELECT (a.*).*, (b.*).*, (v.*).* FROM v AS a(x), v AS b(y), v")]
        fn occurrence_aliases_do_not_modify_other_occurrences(#[case] sql: &str) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            for index in 0..3 {
                let column = query.result_columns().get_column_at(index).unwrap();
                // Names of star outputs are covered separately; this checks each
                // occurrence's retained source binding and selected field type.
                assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            }
            test_helpers::assert_catalog_view_field(&context);
            assert_eq!(test_helpers::row_levels(query), vec![0, 0, 0]);
        }

        #[rstest::rstest]
        #[case::view("SELECT (SELECT (r.*).x) FROM v AS r(x)", 1)]
        #[case::derived("SELECT (SELECT (r.*).x) FROM (SELECT 1 AS n) AS r(x)", 1)]
        #[case::shadow("SELECT (SELECT (r.*).y FROM v AS r(y)) FROM v AS r(x)", 0)]
        fn occurrence_aliases_follow_scope(#[case] sql: &str, #[case] level: usize) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            assert_eq!(test_helpers::row_levels(query), vec![level]);
        }

        #[test]
        fn occurrence_alias_hides_original_relation() {
            let (query, context) = test_helpers::alias_query("SELECT (v.*).n FROM v AS r(x)");
            assert_eq!(context.problems().len(), 1, "{:?}", context.problems());
            assert_eq!(context.problems()[0].code(), "column_source_not_found");
            assert!(expression(&query).get_type().is_none());
        }

        #[rstest::rstest]
        #[case::view("SELECT (r.*).* FROM w AS r(x)")]
        #[case::derived("SELECT (r.*).* FROM (SELECT 1 AS n, 'a'::text AS label) AS r(x)")]
        fn partial_alias_list_preserves_field_order_and_types(#[case] sql: &str) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            for (index, (name, ty)) in [
                ("x", PgBuiltInType::int4()),
                ("label", PgBuiltInType::text()),
            ]
            .into_iter()
            .enumerate()
            {
                let column = query.result_columns().get_column_at(index).unwrap();
                assert_eq!(column.column_name().as_ref().unwrap().as_str(), name);
                assert_eq!(column.col_type(), &Some(ty));
            }
            assert!(query.result_columns().get_column_at(2).is_none());
        }

        #[rstest::rstest]
        #[case::direct("SELECT (r.*).x FROM t AS r(x)", true)]
        #[case::outer("SELECT (r.*).x FROM t AS a LEFT JOIN t AS r(x) ON false", false)]
        #[case::outer_star("SELECT (r.*).* FROM t AS a LEFT JOIN t AS r(x) ON false", false)]
        fn occurrence_member_preserves_source_nullability(
            #[case] sql: &str,
            #[case] not_null: bool,
        ) {
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            assert_eq!(expression(&query).is_not_null(), Some(not_null));
        }

        #[test]
        fn nominal_function_boundary_retains_argument_binding() {
            let (query, context) =
                test_helpers::alias_query("SELECT identity_v(r.*) FROM v AS r(x)");
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            let call = expression(&query).as_function_call().unwrap();
            assert!(call.overload().is_some());
            let nominal =
                TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
            assert_eq!(expression(&query).get_type(), Some(nominal.clone()));
            let argument = &call.args()[0];
            assert_eq!(argument.value().get_type(), Some(nominal.clone()));
            assert_eq!(
                argument.arg_definition().as_ref().unwrap().get_type(),
                Some(&nominal)
            );
            let row = argument.value().as_whole_row().unwrap();
            assert_eq!(row.alias().unwrap().as_str(), "r");
            assert_eq!(row.relation(), Some(&FullName::with_schema("public", "v")));
            assert_eq!(row.levels_up(), Some(0));
            assert!(row.failure_code().is_none());
            test_helpers::assert_catalog_view_field(&context);
        }

        #[rstest::rstest]
        #[case::member("SELECT (r.*).x FROM (SELECT $1::integer AS n) r(x)")]
        #[case::star("SELECT (r.*).* FROM (SELECT $1::integer AS n) r(x)")]
        fn renamed_parameter_fields_retain_constraints(#[case] sql: &str) {
            use crate::context::ParseContext;
            let (query, context) = test_helpers::alias_query(sql);
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            let constraint = context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint();
            assert!(constraint.allows(&PgBuiltInType::int4()));
            let (_, control) =
                test_helpers::alias_query("SELECT (r.*).n FROM (SELECT $1::integer AS n) r");
            assert_eq!(
                constraint,
                control
                    .parameter_constraints()
                    .parameter(1)
                    .unwrap()
                    .constraint(),
            );
            assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
            assert_eq!(test_helpers::row_levels(query), vec![0]);
        }

        #[rstest::rstest]
        #[case::member(
            "SELECT (r.*).x FROM identity(1) r(x)",
            "member_access_requires_composite_type"
        )]
        #[case::star(
            "SELECT (r.*).* FROM identity(1) r(x)",
            "query_star_projection_requires_composite_type"
        )]
        fn scalar_source_fields_do_not_make_a_composite(#[case] sql: &str, #[case] code: &str) {
            let (query, context) = catalog_query(sql);
            assert_eq!(context.reported_problem_count(), 1);
            test_helpers::assert_problem(&context, code);
            assert!(
                query
                    .result_columns()
                    .get_column_at(0)
                    .unwrap()
                    .col_type()
                    .is_none()
            );
        }
    }

    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::OperatorName,
        sem::{
            AnalysisError, ColumnReferenceExpr, PgBuiltInType, QueryGrouping, QueryNameResolution,
            QueryProblem as SemQueryProblem, SelectStatement, SemScalarExpr,
            TypeReference as SemTypeReference,
            parameter_resolution::{
                ParameterResolution, inference_runs as parameter_resolution_inference_runs,
            },
        },
        test_helpers::{
            create_cache_context,
            query::{output_index, output_type, query, resolve},
        },
    };

    #[test]
    fn retains_query_and_select_structure() {
        let query = query(
            "SELECT DISTINCT ON (n) n FROM (SELECT 1 AS n) s GROUP BY DISTINCT GROUPING SETS ((n), ()) HAVING true WINDOW w AS (ORDER BY n) ORDER BY n DESC NULLS FIRST FETCH FIRST 2 ROWS WITH TIES",
        );
        let query = query.query_clauses().unwrap();
        assert!(query.with_ties());
        let order = &query.order()[0];
        assert_eq!(order.descending(), Some(true));
        assert_eq!(order.nulls_first(), Some(true));
        assert_eq!(output_index(order.expression()), Some(0));
        let select = query.body().select_clauses().unwrap();
        assert_eq!(select.distinct().unwrap().len(), 1);
        assert!(select.group_distinct());
        assert_matches::assert_matches!(&select.group()[0], QueryGrouping::Sets(groups) if groups.len() == 2);
        assert!(select.having().is_some());
        assert_eq!(select.windows()[0].name(), Some("w"));
    }

    #[rstest::rstest]
    #[case::order("SELECT 1 ORDER BY $7 + 1", 7, PgBuiltInType::int4())]
    #[case::group("SELECT 1 GROUP BY $7 + 1", 7, PgBuiltInType::int4())]
    #[case::having("SELECT 1 HAVING $7", 7, PgBuiltInType::bool())]
    #[case::limit("SELECT 1 LIMIT $7", 7, PgBuiltInType::int8())]
    #[case::offset("SELECT 1 OFFSET $7", 7, PgBuiltInType::int8())]
    #[case::distinct("SELECT DISTINCT ON ($7 + 1) 1", 7, PgBuiltInType::int4())]
    #[case::window_partition("SELECT sum(1) OVER (PARTITION BY $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::window_order(
        "SELECT sum(1) OVER w WINDOW w AS (ORDER BY $7 + 1)",
        7,
        PgBuiltInType::int4()
    )]
    #[case::frame(
        "SELECT sum(1) OVER (ORDER BY 1 ROWS $7 PRECEDING)",
        7,
        PgBuiltInType::int8()
    )]
    #[case::filter("SELECT sum(1) FILTER (WHERE $7)", 7, PgBuiltInType::bool())]
    #[case::aggregate_order("SELECT sum(1 ORDER BY $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::within_group(
        "SELECT mode() WITHIN GROUP (ORDER BY $7 + 1)",
        7,
        PgBuiltInType::int4()
    )]
    #[case::nested_order("SELECT 1 ORDER BY (SELECT $7 + 1)", 7, PgBuiltInType::int4())]
    #[case::compound_limit("SELECT 1 UNION ALL SELECT 2 LIMIT $7", 7, PgBuiltInType::int8())]
    #[case::operand_limit("(SELECT 1 LIMIT $7) UNION ALL SELECT 2", 7, PgBuiltInType::int8())]
    #[case::values("VALUES (1), ($7 + 1)", 7, PgBuiltInType::int4())]
    fn infers_parameters_outside_projection(
        #[case] sql: &str,
        #[case] position: u32,
        #[case] ty: SemTypeReference,
    ) {
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{sql}: {:?}",
            context.reported_problems()
        );
        let parameters = context.parameter_constraints();
        let domain = parameters
            .parameter(position)
            .expect("retained numbered parameter")
            .constraint();
        assert!(!domain.is_any());
        assert!(domain.allows(&ty), "{sql}: {domain:?}");
        assert!(!domain.allows(&PgBuiltInType::text()));
    }

    #[rstest::rstest]
    #[case::order("SELECT n AS x FROM (SELECT 1 AS n) s ORDER BY x", Some(0))]
    #[case::ordinal("SELECT 1 AS x ORDER BY 1", Some(0))]
    #[case::input_precedence("SELECT 1 AS n FROM (SELECT true AS n) s GROUP BY n", None)]
    fn binds_aliases_and_ordinals(#[case] sql: &str, #[case] expected: Option<usize>) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let query = query.query_clauses().unwrap();
        let expression = if let Some(order) = query.order().first() {
            order.expression()
        } else {
            let QueryGrouping::Expression(expression) =
                &query.body().select_clauses().unwrap().group()[0]
            else {
                panic!("grouping expression");
            };
            expression
        };
        assert_eq!(output_index(expression), expected);
    }

    #[rstest::rstest]
    #[case::missing_order("SELECT 1 ORDER BY missing", false)]
    #[case::ambiguous_order("SELECT 1 AS x, 2 AS x ORDER BY x", true)]
    #[case::having_has_no_alias("SELECT 1 AS x HAVING x IS NULL", false)]
    fn preserves_failed_column_resolution(#[case] sql: &str, #[case] ambiguous: bool) {
        let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert!(context.reported_problem_count() > 0);
        struct Failures {
            missing: usize,
            ambiguous: usize,
        }
        impl crate::sem::traversal::SemanticVisitorMut for Failures {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                match expression {
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_)) => {
                        self.missing += 1
                    }
                    SemScalarExpr::ColumnRef(ColumnReferenceExpr::AmbiguousColumnRef(_)) => {
                        self.ambiguous += 1
                    }
                    _ => {}
                }
                Ok(())
            }
        }
        let mut failures = Failures {
            missing: 0,
            ambiguous: 0,
        };
        query.walk(&mut failures).unwrap();
        if ambiguous {
            assert_eq!(failures.ambiguous, 1);
        } else {
            assert_eq!(failures.missing, 1);
        }
    }

    #[rstest::rstest]
    #[case::found(
        "SELECT sum(1) OVER w WINDOW w AS ()",
        QueryNameResolution::WindowAlias(0),
        0
    )]
    #[case::missing("SELECT sum(1) OVER w", QueryNameResolution::NotFound, 1)]
    #[case::ambiguous(
        "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
        QueryNameResolution::Ambiguous,
        2
    )]
    fn records_window_resolution(
        #[case] sql: &str,
        #[case] expected: QueryNameResolution,
        #[case] problems: usize,
    ) {
        let (mut query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), problems);
        struct Windows(Vec<QueryNameResolution>);
        impl crate::sem::traversal::SemanticVisitorMut for Windows {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::FuncCall(call) = expression {
                    self.0.push(
                        call.modifiers()
                            .over()
                            .unwrap()
                            .reference()
                            .unwrap()
                            .resolution()
                            .clone(),
                    );
                    assert_matches::assert_matches!(
                        call.name_resolution(),
                        QueryNameResolution::Function(_)
                    );
                }
                Ok(())
            }
        }
        let mut windows = Windows(Vec::new());
        query.walk(&mut windows).unwrap();
        assert_eq!(windows.0, [expected]);
    }

    #[test]
    fn compound_boundary_owns_its_trailing_clauses() {
        let query = query(
            "(SELECT 1 AS n LIMIT 1) UNION ALL (SELECT 2 LIMIT 2) ORDER BY n LIMIT 3 OFFSET 4",
        );
        let boundary = query.query_clauses().unwrap();
        assert!(boundary.set_all());
        assert_eq!(boundary.order().len(), 1);
        assert!(boundary.limit().is_some());
        assert!(boundary.offset().is_some());
        let SelectStatement::Set(set) = boundary.body() else {
            panic!("compound body");
        };
        assert!(set.left().query_clauses().unwrap().order().is_empty());
        assert!(set.right().query_clauses().unwrap().order().is_empty());
    }

    #[test]
    fn ctes_are_owned_once_in_declaration_order() {
        let (mut query, context) = resolve(
            "WITH a AS (SELECT $1 + 1 AS n), b AS (SELECT n FROM a) SELECT n FROM b ORDER BY n",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let [first, second] = query.query_clauses().unwrap().with() else {
            panic!("two local CTE declarations");
        };
        assert_eq!(first.name(), "a");
        assert_eq!(second.name(), "b");
        struct Parameters(usize);
        impl crate::sem::traversal::SemanticVisitorMut for Parameters {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if matches!(expression, SemScalarExpr::Param(_)) {
                    self.0 += 1;
                }
                Ok(())
            }
        }
        let mut parameters = Parameters(0);
        query.walk(&mut parameters).unwrap();
        assert_eq!(parameters.0, 1);
        assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::int4()));
    }

    #[rstest::rstest]
    #[case::clauses("SELECT 1 AS n GROUP BY n HAVING true ORDER BY n LIMIT 1 OFFSET 0")]
    #[case::nested("WITH a AS (SELECT 1 AS n ORDER BY n) SELECT n FROM a ORDER BY (SELECT 1)")]
    #[case::compound("SELECT 1 UNION ALL SELECT 2 ORDER BY 1 LIMIT 1")]
    fn parameter_free_queries_skip_inference(#[case] sql: &str) {
        let before = parameter_resolution_inference_runs();
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(context.parameter_constraints().parameter(1).is_none());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::order("SELECT 1 ORDER BY CAST(?7 AS INTEGER)")]
    #[case::group("SELECT 1 GROUP BY CAST(?7 AS INTEGER)")]
    #[case::having("SELECT 1 GROUP BY 1 HAVING ?7 IS NULL")]
    #[case::limit("SELECT 1 LIMIT ?7")]
    #[case::offset("SELECT 1 LIMIT 1 OFFSET ?7")]
    #[case::compound("SELECT 1 UNION ALL SELECT 2 LIMIT ?7")]
    fn sqlite_retains_clause_parameters(#[case] sql: &str) {
        let (_, context) = resolve(sql, Dialect::sqlite(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert!(context.parameter_constraints().parameter(7).is_some());
    }

    #[rstest::rstest]
    #[case::inherited_numeric(
        "SELECT sum(1) OVER (w RANGE $7 PRECEDING) WINDOW w AS (ORDER BY 1)",
        PgBuiltInType::int4()
    )]
    #[case::range_date(
        "SELECT sum(1) OVER (ORDER BY $8::date RANGE $7 PRECEDING)",
        crate::sem::TypeReference::from_master_type_name("interval")
    )]
    #[case::using("SELECT 1 ORDER BY $7 USING <", PgBuiltInType::int4())]
    fn uses_clause_specific_constraints(#[case] sql: &str, #[case] ty: SemTypeReference) {
        let (_, context) = resolve(sql, Dialect::postgres(), true).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let domain = context
            .parameter_constraints()
            .parameter(7)
            .unwrap()
            .constraint();
        assert!(!domain.is_any());
        assert!(domain.allows(&ty));
        assert!(!domain.allows(&PgBuiltInType::text()));
    }

    #[test]
    fn retains_function_modifiers_after_inference_without_duplicate_operands() {
        let (mut query, context) = resolve(
            "SELECT sum($1 + 1 ORDER BY $2 + 1) FILTER (WHERE $3) OVER w WINDOW w AS ()",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        struct Calls {
            functions: usize,
            parameters: Vec<u32>,
        }
        impl crate::sem::traversal::SemanticVisitorMut for Calls {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Param(parameter) = expression {
                    self.parameters.push(parameter.position());
                }
                if let SemScalarExpr::Constrained(value) = expression
                    && let Some(binding) = value.function_binding()
                {
                    self.functions += 1;
                    assert_matches::assert_matches!(
                        binding.resolution(),
                        QueryNameResolution::Function(_)
                    );
                    assert_matches::assert_matches!(
                        binding
                            .modifiers()
                            .over()
                            .unwrap()
                            .reference()
                            .unwrap()
                            .resolution(),
                        QueryNameResolution::WindowAlias(0)
                    );
                    assert_eq!(
                        binding.modifiers().filter().unwrap().get_type(),
                        Some(PgBuiltInType::bool())
                    );
                }
                Ok(())
            }
        }
        let mut calls = Calls {
            functions: 0,
            parameters: Vec::new(),
        };
        query.walk(&mut calls).unwrap();
        calls.parameters.sort();
        assert_eq!(calls.parameters, [1, 2, 3]);
        assert_eq!(calls.functions, 1);
    }

    #[test]
    fn retains_declared_destination_and_lock_bindings() {
        let query =
            query("SELECT n INTO TEMP TABLE destination FROM (SELECT 1 AS n) s FOR UPDATE OF s");
        let boundary = query.query_clauses().unwrap();
        let destination = boundary
            .body()
            .select_clauses()
            .unwrap()
            .destination()
            .unwrap();
        assert_eq!(destination.name().local_name(), "destination");
        assert_eq!(destination.persistence(), "t");
        assert_matches::assert_matches!(
            boundary.locks()[0].relations()[0].resolution(),
            QueryNameResolution::SourceAlias(_)
        );
    }

    #[test]
    fn missing_window_inheritance_is_an_explicit_failed_lookup() {
        let (query, context) = resolve(
            "SELECT sum(1) OVER w WINDOW w AS (missing)",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let window = &query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .windows()[0];
        assert_matches::assert_matches!(
            window.reference().unwrap().resolution(),
            QueryNameResolution::NotFound
        );
    }

    #[test]
    fn parameter_conflicts_across_clauses_produce_diagnostics() {
        let (_, context) =
            resolve("SELECT 1 HAVING $1 LIMIT $1", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problems().len(), 1);
        assert_eq!(
            context.reported_problems()[0].code(),
            "parameter_constraint"
        );
    }

    #[test]
    fn values_refresh_result_metadata_after_inference() {
        let (query, context) = resolve("VALUES ($1 + 1)", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(output_type(&query, "column1"), Some(PgBuiltInType::int4()));
    }

    #[test]
    fn inference_uses_captured_function_and_operator_bindings() {
        let (mut query, mut catalog) = resolve(
            "SELECT sum($1 + 1) FILTER (WHERE $2) ORDER BY $3 + 1",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        catalog.setup_function("sum", &[]);
        catalog.set_get_operator_op_result(&OperatorName::from_parts(None, "+"), true, true, None);
        catalog.begin_parameter_type_analysis();
        let constraints =
            ParameterResolution::infer_query(&mut query, &mut catalog, &mut create_cache_context())
                .unwrap();
        catalog.finish_analysis(Ok(())).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert!(
            constraints
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(!constraints.parameter(1).unwrap().constraint().is_any());
        assert_eq!(
            constraints
                .parameter(2)
                .unwrap()
                .constraint()
                .admissible_types(),
            Some([PgBuiltInType::bool()].as_slice())
        );
        assert!(!constraints.parameter(3).unwrap().constraint().is_any());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::right_alias("SELECT 1 AS n UNION ALL SELECT 2 AS m ORDER BY m")]
    #[case::right_expression(
        "SELECT CAST(1 AS TEXT) UNION ALL SELECT CAST(2 AS TEXT) ORDER BY CAST(2 AS TEXT)"
    )]
    #[case::duplicate_alias("SELECT 1 AS n, 2 AS n ORDER BY n")]
    fn sqlite_binds_ordering_to_its_output(#[case] sql: &str) {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        connection.prepare(sql).unwrap();
        let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(
            output_index(query.query_clauses().unwrap().order()[0].expression()),
            Some(0)
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::forward(
        "SELECT 1 WINDOW w AS (later), later AS ()",
        QueryNameResolution::WindowAlias(1)
    )]
    #[case::missing_base("SELECT 1 WINDOW w AS (missing)", QueryNameResolution::NotFound)]
    fn sqlite_window_inheritance_records_dialect_binding(
        #[case] sql: &str,
        #[case] expected: QueryNameResolution,
    ) {
        let connection = ::rusqlite::Connection::open_in_memory().unwrap();
        connection.prepare(sql).unwrap();
        let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let clauses = query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap();
        assert_eq!(
            clauses.windows()[0].reference().unwrap().resolution(),
            &expected
        );
    }

    #[rstest::rstest]
    #[case::missing_operator("SELECT 1 ORDER BY 1 USING #", false)]
    #[case::missing_lock("SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent", true)]
    fn retains_failed_clause_name_bindings(#[case] sql: &str, #[case] lock: bool) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let boundary = query.query_clauses().unwrap();
        let outcome = if lock {
            boundary.locks()[0].relations()[0].resolution()
        } else {
            boundary.order()[0].using().unwrap().resolution()
        };
        assert_matches::assert_matches!(outcome, QueryNameResolution::NotFound);
    }

    #[test]
    fn failed_function_name_lookup_survives_inference() {
        let (mut query, context) = resolve("SELECT absent($1)", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        struct Bindings(usize);
        impl crate::sem::traversal::SemanticVisitorMut for Bindings {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::Constrained(value) = expression
                    && let Some(binding) = value.function_binding()
                {
                    assert_matches::assert_matches!(
                        binding.resolution(),
                        QueryNameResolution::NotFound
                    );
                    self.0 += 1;
                }
                Ok(())
            }
        }
        let mut bindings = Bindings(0);
        query.walk(&mut bindings).unwrap();
        assert_eq!(bindings.0, 1);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_window_definitions_do_not_bind_projection_aliases() {
        let (query, context) = resolve(
            "SELECT 1 AS n WINDOW w AS (ORDER BY n)",
            Dialect::sqlite(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let windows = query
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .windows();
        assert_matches::assert_matches!(
            windows[0].order()[0].expression(),
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::UndefinedColumn(_))
        );
    }

    #[rstest::rstest]
    #[case::duplicate_window(
    "SELECT 1 WINDOW w AS (), w AS ()",
    vec![crate::sem::QueryProblem::duplicate_window_definition("w")]
)]
    #[case::missing_window(
    "SELECT sum(1) OVER absent",
    vec![crate::sem::QueryProblem::reference_not_found("absent")]
)]
    #[case::ambiguous_window(
    "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
    vec![crate::sem::QueryProblem::duplicate_window_definition("w"), crate::sem::QueryProblem::ambiguous_reference("w")]
)]
    #[case::missing_operator(
    "SELECT 1 ORDER BY 1 USING #",
    vec![crate::sem::QueryProblem::reference_not_found("#")]
)]
    #[case::missing_lock_source(
    "SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent",
    vec![crate::sem::QueryProblem::reference_not_found("absent")]
)]
    #[case::zero_position(
    "SELECT 1 ORDER BY 0",
    vec![crate::sem::QueryProblem::position_out_of_range(0)]
)]
    #[case::group_position(
    "SELECT 1 GROUP BY 2",
    vec![crate::sem::QueryProblem::position_out_of_range(2)]
)]
    #[case::compound_expression(
    "SELECT 1 UNION SELECT 2 ORDER BY 1 + 1",
    vec![crate::sem::QueryProblem::invalid_compound_order()]
)]
    fn reports_structured_query_problems(
        #[case] sql: &str,
        #[case] expected: Vec<SemQueryProblem>,
    ) {
        let (_, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        let mut actual = Vec::new();
        for problem in context.reported_problems() {
            if let Some(problem) = problem.query_problem() {
                actual.push(problem.clone());
            }
        }
        assert_eq!(actual, expected);
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_in(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_in_body(ast, context, outer))
    }
}

#[cfg(test)]
mod test_name_resolution_phase {
    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        names::OperatorName,
        parse_statements,
        sem::{
            BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, SelectStatement,
            parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case("SELECT CASE WHEN true THEN $1 ELSE 1 END")]
    #[case("SELECT COALESCE($1, 1)")]
    #[case("SELECT ($1)[$2]")]
    #[case("SELECT $1 = ANY($2)")]
    #[case("SELECT $1 = ALL($2)")]
    fn untyped_operands_do_not_prevent_name_resolution(#[case] sql: &str) {
        let dialect = Dialect::postgres();
        let statement = parse_statements(sql, dialect).next().unwrap().unwrap();
        let StatementNode::Query(query) = statement.node else {
            panic!("query fixture expected");
        };
        let mut catalog = TestParseContext::default();
        let int = PgBuiltInType::int4();
        let operator = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
            &int,
            &int,
            &PgBuiltInType::bool(),
            true,
        )]);
        catalog.set_get_operator_op_result(
            &OperatorName::from_parts(None, "="),
            true,
            true,
            Some(&operator),
        );
        let before = parameter_resolution_inference_runs();
        catalog.begin_parameter_type_analysis();
        let result = SelectStatement::from_ast_in(&query, &mut catalog, None);
        catalog.finish_analysis(result).unwrap();
        assert_eq!(catalog.reported_problem_count(), 0);
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
    }
}

impl SelectStatement {
    #[inline(never)]
    pub(crate) fn from_ast_in_body(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::analyze_ast_query(ast, context, outer)
    }
}

impl SelectStatement {
    pub(crate) fn analyze_ast_query(
        ast: &Query,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let with = WithClause::from_ast(&ast.with, context, outer)?;
        let restricted = if Self::ast_is_compound(&ast.body)
            && outer.is_some_and(|scope| scope.direct_rule_identity().is_some())
        {
            outer.map(ScalarAnalysisContext::without_rule_rows)
        } else {
            None
        };
        let query_id = context.analysis_state_mut().grouping_mut().new_query();
        let body = Self::from_ast_body(
            &ast.body,
            context,
            &with,
            restricted.as_ref().or(outer),
            query_id,
        )?;
        if body.select_clauses().is_none() {
            context.analysis_state_mut().grouping_mut().set_keys(
                query_id,
                &[],
                body.result_columns(),
            )?;
        }
        let empty = FromClause::default();
        let order_outer = if Self::ast_is_compound(&ast.body) {
            None
        } else {
            outer
        };
        let mut scope = ScalarAnalysisContext::nested(
            &with,
            body.source_scope().unwrap_or(&empty),
            order_outer,
        );
        scope.set_grouping_query(query_id);
        scope.set_windows(Self::ast_window_names(&ast.body));
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(body.result_columns());
        }
        let binding = QueryOrder::ast_order_binding(&ast.body);
        let order = QueryOrder::from_ast_for_query(
            &ast.order,
            &ast.body,
            context,
            &scope,
            body.result_columns(),
            binding,
        )?;
        let row_scope = ScalarAnalysisContext::nested(&with, &empty, outer);
        let row_scope = row_scope.with_grouping_placement("LIMIT/OFFSET");
        let limit = SemScalarExpr::from_ast_optional(&ast.limit, context, &row_scope)?;
        let offset = SemScalarExpr::from_ast_optional(&ast.offset, context, &row_scope)?;
        let locks = QueryLock::from_ast_list(&ast.locks, context, &scope)?;
        let local = WithClause::ast_local_definitions(&ast.with, with);
        let mut query = SemanticQuery::new(local, ast.with.recursive, body, order, limit, offset);
        query.set_modifiers(ast.with_ties, Self::ast_set_all(&ast.body), locks);
        let mut query = Self::query(query);
        query.complete_grouping(context)?;
        Ok(query)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_body(
        ast: &QueryBody,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        match ast {
            QueryBody::Select(select) => {
                Self::from_ast_select(select.as_ref(), context, with, outer, query_id)
            }
            QueryBody::Set(set) => Self::from_ast_set(set.as_ref(), context, with, outer),
            QueryBody::Values(rows) => Self::from_ast_values(rows, context, with, outer, query_id),
        }
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_values(
        rows: &[Vec<Expr>],
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let mut scope = ScalarAnalysisContext::nested(with, &from, outer);
        scope.set_grouping_query(query_id);
        let scope = scope.with_grouping_placement("VALUES");
        let mut resolved = Vec::new();
        for row in rows {
            resolved.push(SemScalarExpr::from_ast_list(row, context, &scope)?);
        }
        Ok(Self::values(resolved))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set_operation(
        ast: &SetOperation,
        context: &mut impl ParseContext,
        left: Self,
        right: Self,
    ) -> Result<Self, AnalysisError> {
        let kind = match ast {
            SetOperation::Union => SetOperationKind::union(),
            SetOperation::Intersect => SetOperationKind::intersect(),
            SetOperation::Except => SetOperationKind::except(),
        };
        Self::set_operation(context, kind, left, right)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_select(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<Self, AnalysisError> {
        Self::validate_ast_rule_destination(ast, context, outer)?;
        let from = Self::from_ast_from(ast, context, with, outer, query_id)?;
        let mut scope = Self::ast_scope(ast, with, &from, outer, query_id);
        let windows = Self::from_ast_windows(ast, context, &scope)?;
        let predicate = Self::from_ast_where(ast, context, &scope)?;
        let mut results = Self::from_ast_targets(ast, context, with, &from, &scope)?;
        let distinct = Self::from_ast_distinct(ast, context, &scope, &results)?;
        let group = Self::from_ast_grouping(ast, context, &mut scope, &mut results, query_id)?;
        let having = Self::from_ast_having(ast, context, &scope)?;
        let clauses = Self::from_ast_clauses(ast, distinct, group, having, windows)?;
        Ok(Self::ast_build_result(results, from, predicate, clauses))
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_rule_destination(
        ast: &Select,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<(), AnalysisError> {
        if ast.into.is_some()
            && let Some(scope) = outer
            && let Some(identity) = scope.rule_identity()
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity,
                "SELECT INTO is not allowed in rule actions",
            )))?;
        }
        Ok(())
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_from(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
        query_id: usize,
    ) -> Result<FromClause, AnalysisError> {
        let empty = FromClause::default();
        let mut from_scope = ScalarAnalysisContext::nested(with, &empty, outer);
        from_scope.set_grouping_query(query_id);
        let from_scope = from_scope.with_grouping_placement("FROM clause of its own query level");
        let from = DataSource::from_ast_list_in_scope(&ast.from, context, with, &from_scope)?;
        if let Some(scope) = outer {
            scope.validate_rule_sources(context, &from)?;
        }
        Ok(from)
    }
}

impl SelectStatement {
    pub(crate) fn ast_scope<'a>(
        ast: &Select,
        with: &'a WithClause,
        from: &'a FromClause,
        outer: Option<&ScalarAnalysisContext<'a>>,
        query_id: usize,
    ) -> ScalarAnalysisContext<'a> {
        let mut scope = ScalarAnalysisContext::nested(with, from, outer);
        scope.set_grouping_query(query_id);
        scope.set_windows(QueryWindow::ast_names(&ast.windows));
        scope
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_windows(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<QueryWindow>, AnalysisError> {
        QueryWindow::from_ast_definitions(&ast.windows, context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_where(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(
            &ast.predicate,
            context,
            &scope.with_grouping_placement("WHERE"),
        )
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_targets(
        ast: &Select,
        context: &mut impl ParseContext,
        with: &WithClause,
        from: &FromClause,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let mut results = ResultColumnCollection::default();
        for target in &ast.targets {
            ResultColumnCollection::ast_append_to_results(
                target,
                context,
                with,
                from,
                scope,
                &mut results,
            )?;
        }
        Ok(results)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_distinct(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &ResultColumnCollection,
    ) -> Result<Option<Vec<SemScalarExpr>>, AnalysisError> {
        let distinct = if let Some(distinct) = &ast.distinct {
            let mut resolved = Vec::new();
            for expression in distinct {
                resolved.push(SemScalarExpr::from_ast_clause(
                    expression,
                    context,
                    scope,
                    results,
                    ClauseBinding::Order,
                )?);
            }
            Some(resolved)
        } else {
            None
        };
        Ok(distinct)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_grouping(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &mut ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        query_id: usize,
    ) -> Result<Vec<QueryGrouping>, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            scope.set_output_aliases(results);
        }
        let group = QueryGrouping::from_ast_list(&ast.group, context, scope, results)?;
        for grouping in &group {
            grouping.validate_grouping_targets(results, query_id, context)?;
        }
        context
            .analysis_state_mut()
            .grouping_mut()
            .set_keys(query_id, &group, results)?;
        Ok(group)
    }
}

#[cfg(test)]
mod test_from_ast_grouping {
    // Public semantic outcomes, using the lightweight query catalog.

    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        parse_statements,
        sem::{GroupingExpr, PgBuiltInType, SelectStatement, SemAst, SemScalarExpr},
        test_helpers::{
            create_cache_context as test_helpers_create_cache_context,
            grouping::groupings,
            query::{catalog, grouping_table_context, resolve},
        },
    };

    #[rstest::rstest]
    #[case::ordinary("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::rollup("SELECT GROUPING(b,a,b) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY ROLLUP(a,b)", &[1,0,1])]
    #[case::cube("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY CUBE(a,b)", &[1,0])]
    #[case::sets("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY GROUPING SETS ((a,b),(),(a))", &[1,0])]
    #[case::duplicate_keys("SELECT GROUPING(a,a) FROM (SELECT 1 AS a) s GROUP BY a,a", &[0,0])]
    #[case::later_parameter("SELECT GROUPING(a+1), $2 FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
    #[case::qualified_function("SELECT GROUPING(identity(a)) FROM (SELECT 1 AS a) s GROUP BY public.identity(a)", &[0])]
    #[case::repeated_before_another_key("SELECT GROUPING(b,a) FROM (SELECT 1 AS a, 2 AS b) s GROUP BY a,a,b", &[2,0])]
    #[case::expression("SELECT GROUPING(s.a+1) FROM (SELECT 1 AS a) s GROUP BY a+1", &[0])]
    #[case::alias("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY x", &[0])]
    #[case::ordinal("SELECT a+1 AS x, GROUPING(a+1) FROM (SELECT 1 AS a) s GROUP BY 1", &[0])]
    #[case::input_precedence("SELECT a+1 AS a, GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::coalesce("SELECT COALESCE(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::nested_local_grouping("SELECT sum((SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::group_alias_local_subquery("SELECT (SELECT GROUPING(t.a) FROM (SELECT 2 AS a) t GROUP BY t.a) AS g GROUP BY g", &[0])]
    #[case::constant("SELECT GROUPING(1+1) GROUP BY (1+1)", &[0])]
    #[case::parentheses("SELECT GROUPING((a)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::noop_argument_cast("SELECT GROUPING(CAST(a AS integer)) FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    #[case::noop_key_cast("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY CAST(a AS integer)", &[0])]
    #[case::having("SELECT a FROM (SELECT 1 AS a) s GROUP BY a HAVING GROUPING(a) IS NOT NULL", &[0])]
    #[case::order("SELECT a FROM (SELECT 1 AS a) s GROUP BY a ORDER BY GROUPING(a)", &[0])]
    #[case::window_order("SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a WINDOW w AS (ORDER BY GROUPING(a))", &[0])]
    #[case::window_aggregate("SELECT sum(GROUPING(a)) OVER () FROM (SELECT 1 AS a) s GROUP BY a", &[0])]
    fn binds_grouping_keys(
        #[case] sql: &str,
        #[case] indices: &[usize],
        #[values(false, true)] infer: bool,
    ) {
        let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{sql}: {:?}",
            context.reported_problems()
        );
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert!(grouping.is_valid(), "{sql}: {grouping:?}");
            assert_eq!(grouping.levels_up(), 0);
            assert_eq!(grouping.arguments().len(), indices.len());
            for (argument, index) in grouping.arguments().iter().zip(indices) {
                assert_eq!(argument.key_index(), Some(*index));
            }
        }
    }

    #[rstest::rstest]
    #[case::mismatch(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a+1",
        "query_grouping_argument_not_grouped"
    )]
    #[case::function_missing(
        "SELECT GROUPING(missing(a)) FROM (SELECT 1 AS a) s GROUP BY missing(a)",
        "query_grouping_argument_not_grouped"
    )]
    #[case::both_missing(
        "SELECT GROUPING(missing) GROUP BY missing",
        "query_grouping_argument_not_grouped"
    )]
    #[case::missing(
        "SELECT GROUPING(missing) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::ambiguous(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::where_clause(
        "SELECT a FROM (SELECT 1 AS a) s WHERE GROUPING(a) IS NULL GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_clause(
        "SELECT 1 FROM (SELECT 1 AS a) s GROUP BY GROUPING(a)",
        "query_invalid_grouping_placement"
    )]
    #[case::group_alias(
        "SELECT GROUPING(a) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_ordinal(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY 1,a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_wrapped_alias(
        "SELECT GROUPING(a)+1 AS g FROM (SELECT 1 AS a) s GROUP BY ROLLUP(g,a)",
        "query_invalid_grouping_placement"
    )]
    #[case::group_correlated_alias(
        "SELECT (SELECT GROUPING(s.a)) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
        "query_invalid_grouping_placement"
    )]
    #[case::nested(
        "SELECT GROUPING(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::aggregate_argument(
        "SELECT sum(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::correlated_aggregate_inside(
        "SELECT GROUPING((SELECT sum(s.a))) FROM (SELECT 1 AS a) s GROUP BY (SELECT sum(s.a))",
        "query_invalid_grouping_placement"
    )]
    #[case::aggregate_inside(
        "SELECT GROUPING(sum(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::filter(
        "SELECT sum(a) FILTER (WHERE GROUPING(a) IS NULL) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::frame(
        "SELECT sum(a) OVER (ROWS GROUPING(a) PRECEDING) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::row_limit("SELECT 1 LIMIT GROUPING(1)", "query_invalid_grouping_placement")]
    #[case::no_grouping("SELECT GROUPING(1)", "query_grouping_argument_not_grouped")]
    #[case::inner_constant(
        "SELECT (SELECT GROUPING(1)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::correlated_where(
        "SELECT 1 FROM (SELECT 1 AS a) s WHERE (SELECT GROUPING(s.a)) IS NULL GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::correlated_aggregate(
        "SELECT sum((SELECT GROUPING(s.a))) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    fn reports_grouping_errors(#[case] sql: &str, #[case] code: &str) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == code),
            "{sql}: {:?}",
            context.reported_problems()
        );
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert!(!grouping.is_valid(), "{sql}: {grouping:?}");
        }
    }

    #[rstest::rstest]
    #[case::outer("SELECT (SELECT GROUPING(s.a)) FROM (SELECT 1 AS a) s GROUP BY a", 1)]
    #[case::shadowed(
        "SELECT (SELECT GROUPING(s.a) FROM (SELECT 2 AS a) s GROUP BY s.a) FROM (SELECT 1 AS a) s GROUP BY a",
        0
    )]
    #[case::mixed(
        "SELECT (SELECT GROUPING(s.a+t.a) FROM (SELECT 2 AS a) t GROUP BY s.a+t.a) FROM (SELECT 1 AS a) s GROUP BY a",
        0
    )]
    fn binds_the_owning_query(#[case] sql: &str, #[case] levels_up: usize) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        for grouping in groupings(&query) {
            assert!(grouping.is_valid(), "{grouping:?}");
            assert_eq!(grouping.levels_up(), levels_up);
            assert_eq!(grouping.arguments()[0].key_index(), Some(0));
        }
    }

    #[rstest::rstest]
    #[case::correlated_from(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x",
        false,
        1
    )]
    #[case::correlated_join(
        "SELECT * FROM (VALUES (1)) s(a) JOIN LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x ON true",
        false,
        1
    )]
    #[case::local(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a) FROM (VALUES(2)) t(a) GROUP BY t.a) x",
        true,
        0
    )]
    #[case::mixed(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a+s.a) FROM (VALUES(2)) t(a) GROUP BY t.a+s.a) x",
        true,
        0
    )]
    fn binds_grouping_in_lateral_sources(
        #[case] sql: &str,
        #[case] valid: bool,
        #[case] levels_up: usize,
        #[values(false, true)] infer: bool,
    ) {
        let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert_eq!(grouping.is_valid(), valid, "{sql}: {grouping:?}");
            assert_eq!(grouping.levels_up(), levels_up, "{sql}: {grouping:?}");
            assert_eq!(
                grouping.arguments()[0].key_index(),
                if valid { Some(0) } else { None }
            );
        }
        if valid {
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{sql}: {:?}",
                context.reported_problems()
            );
        } else {
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "query_invalid_grouping_placement"),
                "{sql}: {:?}",
                context.reported_problems()
            );
        }
    }

    #[test]
    fn retains_type_nullability_and_parameter_operands() {
        let (query, context) = resolve(
            "SELECT GROUPING(a+$7) FROM (SELECT 1 AS a) s GROUP BY a+$7",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let expression = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap();
        assert_eq!(expression.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(expression.is_not_null(), Some(true));
        assert_eq!(expression.get_column_name(), "grouping");
        let parameter = context.parameter_constraints();
        let constraint = parameter.parameter(7).unwrap().constraint();
        assert!(constraint.allows(&PgBuiltInType::int4()));
        assert!(!constraint.allows(&PgBuiltInType::text()));
        let grouping = expression.as_grouping().unwrap();
        assert!(grouping.is_valid());
        assert_eq!(
            grouping.arguments()[0].expression().get_type(),
            Some(PgBuiltInType::int4())
        );
    }

    #[rstest::rstest]
    #[case::maximum(31, true)]
    #[case::too_many(32, false)]
    fn enforces_argument_limit(#[case] count: usize, #[case] valid: bool) {
        let arguments = vec!["a"; count].join(",");
        let (query, context) = resolve(
            &format!("SELECT GROUPING({arguments}) FROM (SELECT 1 AS a) s GROUP BY a"),
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), usize::from(!valid));
        let groupings = groupings(&query);
        assert_eq!(groupings[0].arguments().len(), count);
        assert_eq!(groupings[0].is_valid(), valid);
    }

    #[test]
    fn rejects_invalid_grouping_through_diagnostic_policy() {
        let statement = parse_statements("SELECT GROUPING(1)", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::Query(query) = statement.node else {
            panic!("query");
        };
        let mut context = catalog();
        context.reject_diagnostics_after(0);
        assert!(
            SelectStatement::from_ast(
                &query,
                &mut context,
                &mut crate::test_helpers::create_cache_context()
            )
            .is_err()
        );
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn grouping_result_type_does_not_constrain_argument_type() {
        let (query, context) = resolve(
            "SELECT GROUPING(CAST($1 AS text)) GROUP BY CAST($1 AS text)",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let grouping = groupings(&query).pop().unwrap();
        assert!(grouping.is_valid());
        assert_eq!(
            grouping.arguments()[0].expression().get_type(),
            Some(PgBuiltInType::text())
        );
        let serialized = ::serde_json::to_string(&grouping).unwrap();
        let restored: GroupingExpr = ::serde_json::from_str(&serialized).unwrap();
        assert!(restored.is_valid());
        assert_eq!(restored.arguments()[0].key_index(), Some(0));
    }

    #[rstest::rstest]
    #[case::insert_value("INSERT INTO t VALUES(GROUPING(1))")]
    #[case::insert_returning("INSERT INTO t VALUES(1) RETURNING GROUPING(a)")]
    #[case::update_value("UPDATE t SET a=GROUPING(a)")]
    #[case::update_returning("UPDATE t SET a=1 RETURNING GROUPING(a)")]
    #[case::delete_where("DELETE FROM t WHERE GROUPING(a) IS NULL")]
    #[case::delete_returning("DELETE FROM t RETURNING GROUPING(a)")]
    fn rejects_grouping_outside_select(#[case] sql: &str) {
        let mut cache_context = test_helpers_create_cache_context();
        let context = grouping_table_context();
        let analysis = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .unwrap();
        assert!(
            analysis
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "query_invalid_grouping_placement")
        );
    }

    #[rstest::rstest]
    #[case::missing("SELECT GROUPING(missing) GROUP BY missing", false)]
    #[case::ambiguous(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
        true
    )]
    fn preserves_failed_argument_lookup(#[case] sql: &str, #[case] ambiguous: bool) {
        let (query, _) = resolve(sql, Dialect::postgres(), false).unwrap();
        let grouping = groupings(&query).pop().unwrap();
        assert!(!grouping.is_valid());
        assert_eq!(grouping.arguments()[0].key_index(), None);
        let expression = grouping.arguments()[0].expression();
        if ambiguous {
            ::assert_matches::assert_matches!(
                expression,
                SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::AmbiguousColumnRef(_))
            );
        } else {
            ::assert_matches::assert_matches!(
                expression,
                SemScalarExpr::ColumnRef(crate::sem::ColumnReferenceExpr::UndefinedColumn(_))
            );
        }
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_having(
        ast: &Select,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        SemScalarExpr::from_ast_optional(&ast.having, context, scope)
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_clauses(
        ast: &Select,
        distinct: Option<Vec<SemScalarExpr>>,
        group: Vec<QueryGrouping>,
        having: Option<SemScalarExpr>,
        windows: Vec<QueryWindow>,
    ) -> Result<SelectClauses, AnalysisError> {
        let mut clauses = SelectClauses::new(distinct, group, ast.group_distinct, having, windows);
        let destination = ast
            .into
            .as_ref()
            .map(SelectDestination::from_ast_destination)
            .transpose()?;
        clauses.set_destination(destination);
        Ok(clauses)
    }
}

impl SelectStatement {
    pub(crate) fn ast_build_result(
        results: ResultColumnCollection,
        from: FromClause,
        predicate: Option<SemScalarExpr>,
        clauses: SelectClauses,
    ) -> Self {
        Self::from(SelectStatementNode::from_resolution(
            results,
            &WithClause::default(),
            from,
            predicate,
            clauses,
        ))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_cte_body(
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
        query: &Query,
    ) -> Result<CteStatement, AnalysisError> {
        let mut query = Self::from_ast_in(query, context, Some(scope))?;
        query.rename_columns(columns);
        Ok(CteStatement::from_query(query))
    }
}

impl SelectStatement {
    pub(crate) fn from_ast_set(
        ast: &SetQuery,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let from = FromClause::default();
        let scope = ScalarAnalysisContext::nested(with, &from, outer);
        let left = Self::from_ast_in(&ast.left, context, Some(&scope))?;
        let right = Self::from_ast_in(&ast.right, context, Some(&scope))?;
        Self::from_ast_set_operation(&ast.operation, context, left, right)
    }
}

impl SelectStatement {
    pub(crate) const fn ast_is_compound(ast: &QueryBody) -> bool {
        matches!(ast, QueryBody::Set(_))
    }
}

impl SelectStatement {
    pub(crate) fn ast_window_names(ast: &QueryBody) -> Vec<String> {
        match ast {
            QueryBody::Select(select) => QueryWindow::ast_names(&select.windows),
            _ => Vec::new(),
        }
    }
}

impl SelectStatement {
    pub(crate) fn ast_set_all(ast: &QueryBody) -> bool {
        match ast {
            QueryBody::Set(set) => set.all,
            _ => false,
        }
    }
}

impl SelectStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &Query,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        for entry in &ast.with.entries {
            CteStatement::validate_ast_select_rule_ctes(&entry.body, identity)?;
        }
        Ok(())
    }
}
