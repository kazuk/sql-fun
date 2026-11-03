//! Whole-row arguments retain source identity and nominal/structural row types.
use self::test_helpers::{catalog_query, expression};
use crate::{
    names::FullName,
    sem::{PgBuiltInType, TypeReference},
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
    let (query, context) = catalog_query("SELECT (SELECT record_arg(x.*)) FROM (SELECT 1 AS n) x");
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
fn occurrence_member_preserves_source_nullability(#[case] sql: &str, #[case] not_null: bool) {
    let (query, context) = test_helpers::alias_query(sql);
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    assert_eq!(expression(&query).is_not_null(), Some(not_null));
}

#[test]
fn nominal_function_boundary_retains_argument_binding() {
    let (query, context) = test_helpers::alias_query("SELECT identity_v(r.*) FROM v AS r(x)");
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    let call = expression(&query).as_function_call().unwrap();
    assert!(call.overload().is_some());
    let nominal = TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
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
    let (_, control) = test_helpers::alias_query("SELECT (r.*).n FROM (SELECT $1::integer AS n) r");
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

mod test_helpers {
    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        names::FullName,
        sem::{
            FunctionParam, OverloadVariant, PgBuiltInType, SelectStatement, SemScalarExpr,
            TypeReference,
        },
        test_helpers::TestParseContext,
    };

    pub(super) fn assert_problem(context: &TestParseContext, code: &str) {
        for problem in context.reported_problems() {
            if problem.code() == code {
                return;
            }
        }
        panic!("missing {code}: {:?}", context.reported_problems());
    }
    pub(super) fn row_levels(mut query: SelectStatement) -> Vec<usize> {
        #[derive(Default)]
        struct RowReferences(Vec<usize>);
        impl crate::sem::traversal::SemanticVisitorMut for RowReferences {
            fn expression(
                &mut self,
                value: &mut SemScalarExpr,
            ) -> Result<(), crate::sem::AnalysisError> {
                if let Some(row) = value.as_whole_row() {
                    self.0.push(row.levels_up().unwrap());
                }
                Ok(())
            }
        }
        let mut references = RowReferences::default();
        query.walk(&mut references).unwrap();
        references.0
    }
    pub(super) fn expression(query: &SelectStatement) -> &SemScalarExpr {
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
    }
    pub(super) fn view_query(sql: &str) -> (SelectStatement, crate::context::SchemaFileContext) {
        schema_query("CREATE VIEW v AS SELECT 1 AS n", sql)
    }
    pub(super) fn alias_query(sql: &str) -> (SelectStatement, crate::context::SchemaFileContext) {
        schema_query(
            "CREATE VIEW v AS SELECT 1 AS n; CREATE TABLE t(n integer NOT NULL); CREATE VIEW w AS SELECT 1 AS n, 'a'::text AS label; CREATE VIEW nested AS SELECT identity_v(v.*) AS payload FROM v",
            sql,
        )
    }
    pub(super) fn assert_catalog_view_field(context: &crate::context::SchemaFileContext) {
        let ty = TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
        let columns = ty.row_columns(context).unwrap();
        assert!(
            columns
                .get_column_def(&crate::names::ColumnName::from("n"))
                .is_some()
        );
        assert!(
            columns
                .get_column_def(&crate::names::ColumnName::from("x"))
                .is_none()
        );
        assert!(
            columns
                .get_column_def(&crate::names::ColumnName::from("y"))
                .is_none()
        );
    }
    fn schema_query(ddl: &str, sql: &str) -> (SelectStatement, crate::context::SchemaFileContext) {
        use crate::context::BaseParseContext;
        let mut cache_context = crate::test_helpers::create_cache_context();
        let mut base = super::super::super::test_helpers::catalog();
        let nominal =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
        base.set_get_implicit_cast_result(
            &nominal,
            &nominal,
            Some(crate::sem::CastDefinition::new(
                crate::sem::CastContext::no_conversion(),
            )),
        );
        base.setup_function(
            "identity_v",
            &[OverloadVariant::new(
                &Some(nominal.clone()),
                &vec![FunctionParam::new_input_param(&None, &Some(nominal), &None)],
                false,
                false,
            )],
        );
        let args = base.get_args().clone();
        let metadata = base.metadata().clone();
        let mut context = crate::context::SchemaFileContext::new(
            std::path::Path::new("whole_rows.sql"),
            &base.freeze(),
            &args,
            &metadata,
        );
        crate::context::MutSourcePathWithSpan::set_source_path_with_span(
            &mut context,
            &crate::sem::SourcePathWithSpan::new(
                "whole_rows.sql",
                &crate::StringSpan::new(0, ddl.len()),
            ),
        );
        for statement in crate::parse_statements(ddl, Dialect::postgres()) {
            context = statement
                .unwrap()
                .analyze(context, &mut cache_context)
                .unwrap()
                .into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        // Exercise actual view application: a stub that only stores a ViewLike
        // would omit the nominal row type registered by SchemaCatalog::store_view.
        let row_type =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "v"), false);
        assert!(row_type.row_columns(&context).is_some());
        let statement = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::Query(query) = statement.node else {
            panic!("query fixture");
        };
        context.begin_parameter_type_analysis();
        let result = query.resolve_in(&mut context, None);
        let result = context
            .infer_query_parameter_types(result, &mut crate::test_helpers::create_cache_context());
        let query = context.finish_analysis(result).unwrap();
        (query, context)
    }
    pub(super) fn catalog_query(sql: &str) -> (SelectStatement, TestParseContext) {
        let mut cache_context = crate::test_helpers::create_cache_context();
        let mut context = super::super::super::test_helpers::catalog();
        for ddl in [
            "CREATE TABLE pg_catalog.pg_attribute(n integer)",
            "CREATE TABLE pg_catalog.pg_type(n integer)",
        ] {
            context = crate::parse_statements(ddl, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap()
                .analyze(context, &mut cache_context)
                .unwrap()
                .into_context();
        }
        let mut inputs = Vec::new();
        for (name, parameter) in [("pg_attribute", "left_row"), ("pg_type", "right_row")] {
            let ty =
                TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", name), false);
            context.setup_type(ty.clone());
            inputs.push(FunctionParam::new_input_param(
                &Some(parameter.to_owned()),
                &Some(ty),
                &None,
            ));
        }
        context.setup_function(
            "type_arg",
            &[OverloadVariant::new(
                &Some(PgBuiltInType::int4()),
                &vec![FunctionParam::new_input_param(
                    &None,
                    &Some(TypeReference::concrete_type_ref(
                        &FullName::with_schema("pg_catalog", "pg_type"),
                        false,
                    )),
                    &None,
                )],
                false,
                false,
            )],
        );
        context.setup_type(TypeReference::from_master_type_name("oid"));
        context.setup_type(TypeReference::from_master_type_name("record"));
        context.setup_aggregate(
            "count",
            &[OverloadVariant::new(
                &Some(PgBuiltInType::int8()),
                &vec![],
                false,
                false,
            )],
        );
        context.setup_function(
            "accept_rows",
            &[OverloadVariant::new(
                &Some(TypeReference::from_master_type_name("oid")),
                &inputs,
                false,
                false,
            )],
        );
        context.setup_function(
            "record_arg",
            &[OverloadVariant::new(
                &Some(PgBuiltInType::int4()),
                &vec![FunctionParam::new_input_param(
                    &None,
                    &Some(TypeReference::from_master_type_name("record")),
                    &None,
                )],
                false,
                false,
            )],
        );
        let statement = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::Query(query) = statement.node else {
            panic!("query fixture");
        };
        context.begin_parameter_type_analysis();
        let result = query.resolve_in(&mut context, None);
        let result = context
            .infer_query_parameter_types(result, &mut crate::test_helpers::create_cache_context());
        let query = context.finish_analysis(result).unwrap();
        (query, context)
    }
}
