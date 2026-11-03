use super::super::test_helpers::{output_type, table_function_query};
use crate::context::ParseContext;
use crate::sem::{ColumnReferenceExpr, PgBuiltInType, SemScalarExpr};

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
    assert!(
        context
            .reported_problems()
            .iter()
            .any(|problem| { problem.code() == "query_star_projection_requires_composite_type" })
    );
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
#[case::too_many_aliases("SELECT 1 FROM pair_fun() r(a,b,c)", "table_function_too_many_aliases")]
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
        fn expression(
            &mut self,
            value: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if let SemScalarExpr::ColumnRef(reference) = value {
                if self.ambiguous {
                    self.found |= matches!(reference, ColumnReferenceExpr::AmbiguousColumnRef(_));
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
    use crate::context::BaseParseContext;
    use crate::{
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
fn preserves_ordinality_and_join_nullability(#[case] sql: &str, #[case] not_null: Option<bool>) {
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
#[case::join("SELECT s.* FROM (SELECT 1 AS id) s CROSS JOIN ROWS FROM(record_fun('') AS(a int)) r")]
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
    use crate::context::BaseParseContext;
    use crate::names::{ColumnName, FullName, TableName};
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
        fn expression(
            &mut self,
            value: &mut SemScalarExpr,
        ) -> Result<(), crate::sem::AnalysisError> {
            if let SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableFuncColumn(column)) = value {
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
                QueryNameResolution::function(&FullName::with_schema("public", "record_fun"))
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
