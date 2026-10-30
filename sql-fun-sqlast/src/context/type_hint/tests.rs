mod test_helpers;

use self::test_helpers::{analyze, context, layer};
#[cfg(feature = "sqlite")]
use crate::ast::Dialect;
use crate::context::BaseParseContext;
use crate::{
    context::{ParameterTypeHintRead, ParameterTypeHintWrite, ParseContext},
    sem::{PgBuiltInType, SchemaName, TypeReference},
};

#[rstest::rstest]
#[case::first(1, Some("int4"))]
#[case::second(2, Some("text"))]
#[case::missing(3, None)]
#[case::zero(0, None)]
#[case::beyond(4, None)]
#[case::maximum(usize::MAX, None)]
fn maps_sql_numbers(#[case] index: usize, #[case] expected: Option<&str>) {
    let mut context = context();
    context
        .setup_typehinting(&[Some("int4"), Some("text"), None])
        .unwrap();
    assert_eq!(
        context.get_parameter_typehint(index),
        expected.map(TypeReference::from_master_type_name)
    );
}

#[rstest::rstest]
#[case::unqualified("int4", "int4")]
#[case::qualified("pg_catalog.int4", "int4")]
#[case::quoted(r#""pg_catalog"."int4""#, "int4")]
#[case::array("int4[]", "_int4")]
#[case::sql_alias("INTEGER", "int4")]
#[case::sql_boolean("BOOLEAN", "bool")]
#[case::sql_array("integer[2][]", "_int4")]
#[case::sql_array_keyword("integer ARRAY[2]", "_int4")]
#[case::unicode(r#"U&"int\0034""#, "int4")]
fn resolves_existing_type_name_spelling(#[case] hint: &str, #[case] expected: &str) {
    let mut context = context();
    context.setup_typehinting(&[Some(hint)]).unwrap();
    assert_eq!(
        context.get_parameter_typehint(1),
        Some(TypeReference::from_master_type_name(expected))
    );
}

#[rstest::rstest]
#[case::unknown("no_such_type")]
#[case::wrong_schema("absent.int4")]
#[case::syntax("int4[")]
#[case::empty("")]
#[case::two_types("int4, text")]
#[case::extra_statement("int4; SELECT 1")]
#[case::typmod("varchar(100)")]
#[case::numeric_typmod("numeric(5,2)")]
#[case::interval_typmod("interval day to second")]
fn failure_is_atomic(#[case] invalid: &str) {
    let mut context = context();
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
    let mut context = context();
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
    let mut context = context();
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
    let mut context = context();
    let args = context.get_args().clone();
    let metadata = context.metadata().clone();
    context.setup_typehinting(&[Some("bool")]).unwrap();
    let base = context.freeze();
    let mut first = layer(&base, &args, &metadata);
    let mut second = layer(&base, &args, &metadata);
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
    let mut context = context();
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
    let mut cache_context = crate::CacheContext::new();
    let mut base = crate::test_helpers::TestParseContext::default().with_dialect(Dialect::sqlite());
    base.set_get_search_path_result(&[SchemaName::from("main")]);
    let expected = TypeReference::concrete_type_ref(
        &crate::sem::FullName::with_schema("main", "DOUBLE PRECISION"),
        false,
    );
    base.setup_type(expected.clone());
    base.setup_typehinting(&[Some(r#""DOUBLE PRECISION""#)])
        .unwrap();
    assert_eq!(base.get_parameter_typehint(1), Some(expected.clone()));
    assert!(base.setup_typehinting(&[Some("pg_catalog.int4")]).is_err());
    let analyzed = crate::parse_statements("SELECT ?1", Dialect::sqlite())
        .next()
        .unwrap()
        .unwrap()
        .analyze(base, &mut cache_context)
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
    let mut context = context();
    context.setup_typehinting(&[Some("int4")]).unwrap();
    let first = analyze("SELECT $1", context);
    let mut context = first.into_context();
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
    let mut cache_context = crate::CacheContext::new();
    let mut context = context();
    context.setup_typehinting(&[hint]).unwrap();
    let statement = crate::parse_statements("SELECT $1 + 1, $1", context.dialect())
        .next()
        .unwrap()
        .unwrap();
    let sql = statement.deparse_sql(context.dialect()).unwrap();
    let analyzed = statement.analyze(context, &mut cache_context).unwrap();
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

#[rstest::rstest]
#[case::unqualified("unknown")]
#[case::qualified("pg_catalog.unknown")]
#[case::quoted(r#""pg_catalog"."unknown""#)]
#[case::uppercase("UNKNOWN")]
fn unknown_hint_leaves_parameter_for_inference(#[case] hint: &str) {
    let mut context = context();
    context.setup_typehinting(&[Some("text")]).unwrap();
    context.setup_typehinting(&[Some(hint)]).unwrap();
    assert_eq!(context.get_parameter_typehint(1), None);
    let analyzed = analyze("SELECT $1 + 1", context);
    assert!(analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
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
}

#[rstest::rstest]
#[case::punctuation(r#""a.b""#, "a.b")]
#[case::quotes(r#""a""b""#, "a\"b")]
#[case::literal_suffix(r#""kind[]""#, "kind[]")]
#[case::quoted_alias(r#""integer""#, "integer")]
fn preserves_quoted_type_identity(#[case] hint: &str, #[case] logical: &str) {
    let mut context = crate::test_helpers::TestParseContext::default();
    context.set_get_search_path_result(&[SchemaName::from("public")]);
    let expected =
        TypeReference::from_full_name(crate::sem::FullName::with_schema("public", logical));
    context.setup_type(expected.clone());
    context.setup_typehinting(&[Some(hint)]).unwrap();
    assert_eq!(context.get_parameter_typehint(1), Some(expected));
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::double_quote(r#""a.b""#, "a.b", false)]
#[case::literal_suffix(r#""kind[]""#, "kind[]", false)]
#[case::backtick("`kind`", "kind", false)]
#[case::bracket("[kind]", "kind", false)]
#[case::multiword("DOUBLE PRECISION", "DOUBLE PRECISION", false)]
#[case::array("kind[]", "kind", true)]
fn preserves_sqlite_type_identity(#[case] hint: &str, #[case] logical: &str, #[case] array: bool) {
    let mut context =
        crate::test_helpers::TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("main")]);
    let element = TypeReference::from_full_name(crate::sem::FullName::with_schema("main", logical));
    context.setup_type(element.clone());
    let expected = if array { element.array_type() } else { element };
    context.setup_typehinting(&[Some(hint)]).unwrap();
    assert_eq!(context.get_parameter_typehint(1), Some(expected));
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_unknown_type_remains_explicit() {
    let mut context =
        crate::test_helpers::TestParseContext::default().with_dialect(Dialect::sqlite());
    context.set_get_search_path_result(&[SchemaName::from("pg_catalog")]);
    let expected = TypeReference::from_master_type_name("unknown");
    context.setup_type(expected.clone());
    context.setup_typehinting(&[Some("unknown")]).unwrap();
    assert_eq!(context.get_parameter_typehint(1), Some(expected));
}
