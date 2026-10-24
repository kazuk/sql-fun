//! Whole-row arguments retain source identity and nominal/structural row types.
use self::test_helpers::{catalog_query, expression};
use crate::sem::{FullName, PgBuiltInType, TypeReference};

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

mod test_helpers {
    use crate::ast::{Dialect, StatementNode};
    use crate::sem::{
        FullName, FunctionParam, OverloadVariant, ParseContext, PgBuiltInType, SelectStatement,
        SemScalarExpr, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

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
        impl crate::sem::traversal::SemanticVisitor for RowReferences {
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
    pub(super) fn view_query(sql: &str) -> (SelectStatement, crate::sem::SchemaFileContext) {
        use crate::sem::BaseParseContext;
        let base = super::super::super::test_helpers::catalog();
        let args = base.get_args().clone();
        let metadata = base.metadata().clone();
        let mut context = crate::sem::SchemaFileContext::new(
            std::path::Path::new("whole_rows.sql"),
            &base.freeze(),
            &args,
            &metadata,
        );
        let ddl = crate::parse_statements("CREATE VIEW v AS SELECT 1 AS n", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        context = ddl.analyze(context).unwrap().into_context();
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
        let result = context.infer_query_parameter_types(result);
        let query = context.finish_analysis(result).unwrap();
        (query, context)
    }
    pub(super) fn catalog_query(sql: &str) -> (SelectStatement, TestParseContext) {
        let mut context = super::super::super::test_helpers::catalog();
        for ddl in [
            "CREATE TABLE pg_catalog.pg_attribute(n integer)",
            "CREATE TABLE pg_catalog.pg_type(n integer)",
        ] {
            context = crate::parse_statements(ddl, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap()
                .analyze(context)
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
        let result = context.infer_query_parameter_types(result);
        let query = context.finish_analysis(result).unwrap();
        (query, context)
    }
}
