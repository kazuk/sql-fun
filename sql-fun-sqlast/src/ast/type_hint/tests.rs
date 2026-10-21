mod test_helpers;

use self::test_helpers::{analyze, context, layer};
#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
use crate::sem::{
    BaseParseContext, ParameterTypeHintRead, ParameterTypeHintWrite, ParseContext, PgBuiltInType,
    SchemaName, TypeReference,
};

#[rstest::rstest]
#[case::first(1, Some("int4"))]
#[case::second(2, Some("text"))]
#[case::missing(3, None)]
#[case::zero(0, None)]
#[case::beyond(4, None)]
#[case::maximum(usize::MAX, None)]
fn maps_sql_numbers(#[case] index: usize, #[case] expected: Option<&str>) {
    let context = context();
    context
        .setup_typehinting(&[Some("int4"), Some("text"), None])
        .unwrap();
    assert_eq!(
        context.get_parameter_typehint(index),
        expected.map(TypeReference::from_master_type_name)
    );
}

#[rstest::rstest]
#[case::alias("integer", "int4")]
#[case::qualified("pg_catalog.int4", "int4")]
#[case::quoted(r#""pg_catalog"."int4""#, "int4")]
#[case::array("integer[]", "_int4")]
#[case::bounds("integer[3][4]", "_int4")]
#[case::multiword("double precision", "float8")]
#[case::float_single("float(24)", "float4")]
#[case::float_double("float(53)", "float8")]
#[case::length("varchar(3)", "varchar")]
#[case::precision("numeric(5,2)", "numeric")]
#[case::timestamp("timestamp(2) without time zone", "timestamp")]
#[case::interval("interval day to second(2)", "interval")]
fn resolves_sql_spelling(#[case] hint: &str, #[case] expected: &str) {
    let context = context();
    context.setup_typehinting(&[Some(hint)]).unwrap();
    assert_eq!(
        context.get_parameter_typehint(1),
        Some(TypeReference::from_master_type_name(expected))
    );
}

#[rstest::rstest]
#[case::unknown("no_such_type")]
#[case::wrong_schema("absent.int4")]
#[case::syntax("integer[")]
#[case::empty("")]
#[case::two_types("integer, text")]
#[case::expression("integer) + (1")]
#[case::extra_statement("integer); SELECT CAST(NULL AS text")]
#[case::alias("integer) AS renamed --")]
#[case::nested_cast("integer)::text --")]
fn failure_is_atomic(#[case] invalid: &str) {
    let context = context();
    context
        .setup_typehinting(&[Some("text"), Some("bool")])
        .unwrap();
    let error = context
        .setup_typehinting(&[Some("int4"), Some(invalid)])
        .unwrap_err();
    assert!(error.to_string().contains("parameter $2"));
    assert_eq!(
        context.get_parameter_typehint(1),
        Some(PgBuiltInType::text())
    );
    assert_eq!(
        context.get_parameter_typehint(2),
        Some(PgBuiltInType::bool())
    );
}

#[rstest::rstest]
#[case::empty(&[])]
#[case::all_absent(&[None, None])]
fn replaces_previous_setup(#[case] replacement: &[Option<&str>]) {
    let context = context();
    context
        .setup_typehinting(&[Some("int4"), Some("text")])
        .unwrap();
    context.setup_typehinting(replacement).unwrap();
    assert_eq!(context.get_parameter_typehint(1), None);
    assert_eq!(context.get_parameter_typehint(2), None);
}

#[rstest::rstest]
#[case::standalone("SELECT $1", &[Some("int4")], &[Some("int4")])]
#[case::repeated("SELECT $1, $1", &[Some("int4")], &[Some("int4"), Some("int4")])]
#[case::different("SELECT $1, $2", &[Some("int4"), Some("text")], &[Some("int4"), Some("text")])]
#[case::mixed("SELECT $1, $2 IS NULL", &[Some("int4"), None], &[Some("int4"), Some("bool")])]
#[case::cte("WITH c AS (SELECT $1 AS n) SELECT n, $1 FROM c", &[Some("int4")], &[Some("int4"), Some("int4")])]
fn typed_parameters_survive_full_analysis(
    #[case] sql: &str,
    #[case] hints: &[Option<&str>],
    #[case] types: &[Option<&str>],
) {
    let context = context();
    context.setup_typehinting(hints).unwrap();
    let analyzed = analyze(sql, context);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    for (index, ty) in types.iter().enumerate() {
        assert_eq!(
            query
                .result_columns()
                .get_column_at(index)
                .unwrap()
                .col_type(),
            &ty.map(TypeReference::from_master_type_name)
        );
    }
    assert_eq!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .single_type(),
        Some(PgBuiltInType::int4())
    );
    if hints.len() == 2 && hints[1].is_none() {
        assert!(
            analyzed
                .context()
                .parameter_constraints()
                .parameter(2)
                .unwrap()
                .constraint()
                .is_any()
        );
    }
}

#[test]
fn hinted_type_is_assigned_before_constraint_collection() {
    let mut context = context();
    context.setup_typehinting(&[Some("int4")]).unwrap();
    context.begin_parameter_type_analysis();
    assert_eq!(context.parameter(1).get_type(), Some(PgBuiltInType::int4()));
    assert_eq!(context.parameter(2).get_type(), None);
}

#[test]
fn independent_queries_share_only_the_catalog() {
    let context = context();
    let args = context.get_args().clone();
    let metadata = context.metadata().clone();
    context.setup_typehinting(&[Some("bool")]).unwrap();
    let base = context.freeze();
    let first = layer(&base, &args, &metadata);
    let second = layer(&base, &args, &metadata);
    assert_eq!(first.get_parameter_typehint(1), None);
    first.setup_typehinting(&[Some("int4")]).unwrap();
    second.setup_typehinting(&[Some("text")]).unwrap();
    assert_eq!(
        analyze("SELECT $1", first)
            .context()
            .get_parameter_typehint(1),
        Some(PgBuiltInType::int4())
    );
    assert_eq!(
        analyze("SELECT $1", second)
            .context()
            .get_parameter_typehint(1),
        Some(PgBuiltInType::text())
    );
    let unhinted = layer(&base, &args, &metadata);
    assert_eq!(unhinted.get_parameter_typehint(1), None);
    let analyzed = analyze("SELECT $1 IS NULL", unhinted);
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .is_any()
    );
}

#[test]
fn resolves_custom_types_using_effective_search_path() {
    let mut context = context();
    context = analyze(r#"CREATE TYPE a.kind AS ENUM ('a')"#, context).into_context();
    context = analyze(r#"CREATE TYPE b.kind AS ENUM ('b')"#, context).into_context();
    context.set_search_path(&[SchemaName::from("b"), SchemaName::from("a")]);
    context
        .setup_typehinting(&[Some("kind"), Some("a.kind")])
        .unwrap();
    assert_eq!(
        context.get_parameter_typehint(1),
        Some(TypeReference::concrete_type_ref(
            &crate::sem::FullName::with_schema("b", "kind"),
            false
        ))
    );
    assert_eq!(
        context.get_parameter_typehint(2),
        Some(TypeReference::concrete_type_ref(
            &crate::sem::FullName::with_schema("a", "kind"),
            false
        ))
    );
}

#[test]
fn incompatible_use_reports_semantic_diagnostics() {
    let context = context();
    context.setup_typehinting(&[Some("int4")]).unwrap();
    let analyzed = analyze("SELECT $1 AND true", context);
    assert!(!analyzed.context().problems().is_empty());
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "parameter_constraint")
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_uses_its_type_spelling_and_catalog() {
    let mut base = crate::test_helpers::TestParseContext::default().with_dialect(Dialect::sqlite());
    base.set_get_search_path_result(&[SchemaName::from("main")]);
    let expected = TypeReference::concrete_type_ref(
        &crate::sem::FullName::with_schema("main", "DOUBLE PRECISION"),
        false,
    );
    base.setup_type(expected.clone());
    base.setup_typehinting(&[Some("DOUBLE PRECISION(5,2)")])
        .unwrap();
    assert_eq!(base.get_parameter_typehint(1), Some(expected.clone()));
    assert!(base.setup_typehinting(&[Some("pg_catalog.int4")]).is_err());
    let analyzed = crate::parse_statements("SELECT ?1", Dialect::sqlite())
        .next()
        .unwrap()
        .unwrap()
        .analyze(base)
        .unwrap();
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(expected)
    );
}

#[rstest::rstest]
#[case::integer("int4", "int4")]
#[case::text("text", "text")]
fn hinted_type_selects_existing_function_overload(#[case] hint: &str, #[case] result: &str) {
    let mut context = context();
    for declaration in [
        "CREATE FUNCTION f(integer) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "CREATE FUNCTION f(text) RETURNS text LANGUAGE sql AS 'SELECT NULL::text'",
    ] {
        context = analyze(declaration, context).into_context();
    }
    context.setup_typehinting(&[Some(hint)]).unwrap();
    let analyzed = analyze("SELECT f($1)", context);
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(TypeReference::from_master_type_name(result))
    );
}

#[test]
fn reusing_a_layer_with_empty_setup_restores_unhinted_inference() {
    let context = context();
    context.setup_typehinting(&[Some("int4")]).unwrap();
    let first = analyze("SELECT $1", context);
    let context = first.into_context();
    context.setup_typehinting(&[]).unwrap();
    let second = analyze("SELECT $1 IS NULL", context);
    assert!(
        second
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .is_any()
    );
}

#[rstest::rstest]
#[case::unhinted(None)]
#[case::hinted(Some("int4"))]
fn operator_constraints_and_executable_sql_stay_consistent(#[case] hint: Option<&str>) {
    let context = context();
    context.setup_typehinting(&[hint]).unwrap();
    let statement = crate::parse_statements("SELECT $1 + 1, $1", context.dialect())
        .next()
        .unwrap()
        .unwrap();
    let sql = statement.deparse_sql(context.dialect()).unwrap();
    let analyzed = statement.analyze(context).unwrap();
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    assert_eq!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .single_type(),
        Some(PgBuiltInType::int4())
    );
    assert_eq!(
        statement.deparse_sql(analyzed.context().dialect()).unwrap(),
        sql
    );
    assert!(!sql.to_ascii_lowercase().contains("cast"));
}
