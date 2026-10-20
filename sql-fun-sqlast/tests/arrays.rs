//! ARRAY semantics through the maintained SQL analysis boundary.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{ParseContext, PgBuiltInType, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::integers("ARRAY[1,2]", "_int4")]
#[case::mixed("ARRAY[1,2.5]", "_numeric")]
#[case::mixed_reverse("ARRAY[2.5,1]", "_numeric")]
#[case::nulls("ARRAY[NULL,NULL]", "_text")]
#[case::strings("ARRAY['a','b']", "_text")]
#[case::unknown("ARRAY['1',2]", "_int4")]
#[case::nullable("ARRAY[NULL::int,1]", "_int4")]
#[case::empty("ARRAY[]::integer[]", "_int4")]
#[case::cast_empty("CAST(ARRAY[] AS integer[])", "_int4")]
#[case::parenthesized_empty("(ARRAY[])::integer[]", "_int4")]
#[case::nested_empty("ARRAY[ARRAY[],ARRAY[]]::integer[]", "_int4")]
#[case::nested("ARRAY[ARRAY[1],ARRAY[2.5]]", "_numeric")]
#[case::nested_cast("ARRAY[ARRAY[1.5],ARRAY[2]]::integer[]", "_int4")]
#[case::array_operands("ARRAY[NULL::int[],NULL::int[]]", "_int4")]
#[case::json("ARRAY['{}'::json,'{}'::json]", "_json")]
#[case::different_records("ARRAY[pg_control_system(),pg_control_checkpoint()]", "_record")]
#[case::record_null("ARRAY[pg_control_system(),NULL]", "_record")]
#[case::record_cast("ARRAY[pg_control_system()]::record[]", "_record")]
#[case::preferred("ARRAY[1::numeric,2::float8,3::int4]", "_float8")]
#[case::varchar_first("ARRAY['a'::varchar,'b'::bpchar]", "_varchar")]
#[case::bpchar_first("ARRAY['a'::bpchar,'b'::varchar]", "_bpchar")]
fn metadata_matches_postgres(#[case] expression: &str, #[case] ty: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(&format!("SELECT {expression} AS result"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.col_type(),
        &Some(TypeReference::from_master_type_name(ty))
    );
    assert_eq!(column.is_not_null(), &Some(true));
    Ok(())
}

#[test]
fn ordered_elements_and_coercions_are_retained() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT ARRAY[1,2.5,NULL]", &[])?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let array = column.expression().unwrap().as_array().unwrap();
    assert_eq!(array.values().len(), 3);
    for value in array.values() {
        assert_eq!(
            value.get_type(),
            Some(TypeReference::from_master_type_name("numeric"))
        );
    }
    let serialized = ::serde_json::to_value(array)?;
    assert!(serialized["values"][0].get("ImplicitCast").is_some());
    assert!(serialized["values"][1].get("Const").is_some());
    assert!(serialized["values"][2].get("ImplicitCast").is_some());
    Ok(())
}

#[rstest::rstest]
#[case::empty("ARRAY[]", "array_empty", 0)]
#[case::categories("ARRAY[1,true]", "array_incompatible_types", 2)]
#[case::typed_string("ARRAY['1'::text,2]", "array_incompatible_types", 2)]
#[case::nested_boundary("ARRAY[ARRAY[NULL],ARRAY[1]]", "array_incompatible_types", 2)]
#[case::explicit_bad("ARRAY[true]::date[]", "array_incompatible_types", 1)]
fn invalid_arrays_keep_failure_and_allow_next_statement(
    #[case] expression: &str,
    #[case] code: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    let problem = analyzed
        .context()
        .problems()
        .iter()
        .find(|problem| problem.code() == code)
        .expect("array diagnostic");
    assert!(
        problem
            .render_metadata()
            .unwrap()
            .message()
            .contains("ARRAY")
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let encoded = ::serde_json::to_value(expression)?;
    let array = encoded
        .get("Array")
        .unwrap_or(&encoded["TypeCast"]["arg"]["Array"]);
    assert_eq!(array["values"].as_array().unwrap().len(), count);
    assert_eq!(array["types"], ::serde_json::json!([]));
    let context = analyzed.into_context();
    let next = ::sql_fun_sqlast::parse_statements(
        "SELECT 1 AS n",
        ::sql_fun_sqlast::ast::Dialect::postgres(),
    )
    .next()
    .unwrap()?
    .analyze(context)?;
    assert_eq!(
        next.ast()
            .clone()
            .into_select_statemtnt()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(PgBuiltInType::int4())
    );
    Ok(())
}

#[rstest::rstest]
#[case::mixed("SELECT ARRAY[$1,1] AS result")]
#[case::reverse("SELECT ARRAY[1,$1] AS result")]
#[case::repeated("SELECT ARRAY[$1,$1,1] AS result")]
#[case::subquery("SELECT ARRAY[(SELECT $1),1] AS result")]
#[case::cte("WITH c AS (SELECT ARRAY[$1,1] AS n) SELECT ARRAY[c.n] AS result FROM c")]
#[case::nested("SELECT ARRAY[ARRAY[$1,1],ARRAY[2]] AS result")]
fn parameters_retain_correlated_numeric_sources(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.result_column(0).unwrap().is_any());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_array()
            .is_some()
    );
    Ok(())
}

#[rstest::rstest]
#[case::only("ARRAY[$1]", "_text")]
#[case::null("ARRAY[$1,NULL]", "_text")]
#[case::explicit("ARRAY[$1]::int[]", "_int4")]
fn parameters_default_or_use_explicit_target(
    #[case] expression: &str,
    #[case] ty: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(&format!("SELECT {expression} AS result"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(TypeReference::from_master_type_name(ty))
    );
    Ok(())
}

#[rstest::rstest]
#[case::implicit("SELECT ARRAY[1]", "array")]
#[case::alias("SELECT ARRAY[1] AS chosen", "chosen")]
#[case::empty_cast("SELECT ARRAY[]::int[]", "array")]
#[case::nested_cast("SELECT ARRAY[]::int[]::bigint[]", "array")]
#[case::parameter_cast("SELECT ARRAY[$1]::int[]", "array")]
fn projection_names(#[case] sql: &str, #[case] name: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .column_name()
            .as_ref()
            .unwrap()
            .as_str(),
        name
    );
    Ok(())
}

#[rstest::rstest]
#[case::domain("ARRAY[NULL::amount,NULL::amount]", "public.amount[]")]
#[case::domain_base("ARRAY[NULL::amount,1]", "pg_catalog.int4[]")]
#[case::domain_unknown("ARRAY[NULL::amount,NULL]", "pg_catalog.int4[]")]
#[case::array_domain("ARRAY[NULL::amount_array,NULL::integer[]]", "pg_catalog.int4[]")]
#[case::array_of_domains("ARRAY[NULL::amount[],NULL::integer[]]", "pg_catalog.int4[]")]
#[case::enum_values("ARRAY['low'::mood,'high'::mood]", "public.mood[]")]
#[case::composite("ARRAY[NULL::pair,NULL::pair]", "public.pair[]")]
#[case::nullable_column("ARRAY[n,NULL] FROM items", "pg_catalog.int4[]")]
fn schema_types_and_bindings_survive_frozen_contexts(
    #[case] expression: &str,
    #[case] ty: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(
        &format!("SELECT {expression}"),
        &[
            "CREATE DOMAIN amount AS integer",
            "CREATE DOMAIN amount_array AS integer[]",
            "CREATE TYPE mood AS ENUM ('low','high')",
            "CREATE TYPE pair AS (n integer)",
            "CREATE TABLE items(n integer)",
        ],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.col_type().as_ref().unwrap().to_string(), ty);
    assert_eq!(column.is_not_null(), &Some(true));
    assert_eq!(
        column
            .expression()
            .unwrap()
            .as_array()
            .unwrap()
            .values()
            .len(),
        2
    );
    Ok(())
}

#[test]
fn operands_keep_successful_and_failed_column_bindings() -> testresult::TestResult {
    let analyzed = heavy::analyze(
        "SELECT ARRAY[1,t.n,missing.n] FROM t",
        &["CREATE TABLE t(n integer)"],
    )?;
    assert!(!analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let array = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(array.values().len(), 3);
    assert!(array.values()[1].table_column_reference().is_some());
    let binding = ::serde_json::to_value(&array.values()[2])?;
    assert!(binding["ColumnRef"]["UndefinedAlias"].is_object());
    Ok(())
}

#[test]
fn conflicting_parameter_use_reports_a_constraint_failure() -> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT ARRAY[$1,1] WHERE $1", &[])?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "parameter_constraint"),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[rstest::rstest]
#[case::single("ARRAY[pg_control_system()]", 1)]
#[case::repeated("ARRAY[pg_control_system(),pg_control_system()]", 2)]
#[case::nested("ARRAY[ARRAY[pg_control_system()],ARRAY[pg_control_system()]]", 2)]
fn anonymous_record_arrays_keep_their_element_fields(
    #[case] expression: &str,
    #[case] count: usize,
) -> testresult::TestResult {
    use ::sql_fun_sqlast::sem::TypeInfoRead;
    let analyzed = heavy::analyze(&format!("SELECT {expression}"), &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let ty = column.col_type().as_ref().unwrap();
    assert!(ty.is_dynamic());
    assert_eq!(ty.is_array(), Some(true));
    let element = analyzed.context().get_array_item_type(ty).unwrap();
    assert!(element.is_record());
    assert_eq!(
        element
            .type_definition()
            .unwrap()
            .get_member_type(analyzed.context(), "pg_control_version"),
        Some(PgBuiltInType::int4())
    );
    let array = column.expression().unwrap().as_array().unwrap();
    assert_eq!(array.values().len(), count);
    let encoded = ::serde_json::to_value(array)?;
    assert!(!encoded.is_null());
    Ok(())
}

#[rstest::rstest]
#[case::subscript("SELECT (ARRAY[1,2])[1]")]
#[case::array_function("SELECT array_length(ARRAY[1,2],1)")]
#[case::record_array_function("SELECT array_length(ARRAY[pg_control_system()],1)")]
fn constructed_types_reach_existing_consumers(#[case] sql: &str) -> testresult::TestResult {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
    Ok(())
}

#[rstest::rstest]
#[case::scalar("pg_control_system()", "control")]
#[case::array("ARRAY[pg_control_system()]", "control[]")]
fn anonymous_records_do_not_implicitly_become_named_composites(
    #[case] expression: &str,
    #[case] target: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(
        &format!("SELECT f({expression})"),
        &[
            "CREATE TYPE control AS (system_identifier bigint, pg_control_version integer, catalog_version_no integer, pg_control_last_modified timestamptz)",
            &format!("CREATE FUNCTION f(value {target}) RETURNS int LANGUAGE sql AS 'SELECT 1'"),
        ],
    )?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| { problem.code() == "function_overload_resolution_failed" }),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[test]
fn subscripted_anonymous_record_has_no_sql_field_descriptor() -> testresult::TestResult {
    let analyzed = heavy::analyze(
        "SELECT ((ARRAY[pg_control_system()])[1]).pg_control_version",
        &[],
    )?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "composite_type_field_not_found"),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[test]
fn parameter_with_anonymous_records_uses_the_record_family() -> testresult::TestResult {
    let analyzed = heavy::analyze(
        "SELECT ARRAY[$1,pg_control_system(),pg_control_checkpoint()]",
        &[],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.col_type(),
        &Some(TypeReference::from_master_type_name("_record"))
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .allows(&BTreeMap::from([(
                1,
                TypeReference::from_master_type_name("record")
            )]))
    );
    Ok(())
}

#[rstest::rstest]
#[case::anonymous("f(pg_control_system())", "text")]
#[case::named("f(NULL::control)", "int4")]
#[case::anonymous_array("fa(ARRAY[pg_control_system()])", "text")]
#[case::named_array("fa(NULL::control[])", "int4")]
#[case::constructed_named_array("fa(ARRAY[NULL::control])", "int4")]
#[case::anonymous_polymorphic("fp(pg_control_system())", "text")]
fn competing_record_overloads_select_postgres_candidate(
    #[case] expression: &str,
    #[case] result: &str,
) -> testresult::TestResult {
    // Real builtin metadata supplies pg_control_system's anonymous row type.
    let analyzed = heavy::analyze(
        &format!("SELECT {expression}"),
        &[
            "CREATE TYPE control AS (system_identifier bigint, pg_control_version integer, catalog_version_no integer, pg_control_last_modified timestamptz)",
            "CREATE FUNCTION f(value record) RETURNS text LANGUAGE plpgsql AS 'BEGIN RETURN NULL; END'",
            "CREATE FUNCTION f(value control) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            "CREATE FUNCTION fa(value anyarray) RETURNS text LANGUAGE sql AS 'SELECT NULL::text'",
            "CREATE FUNCTION fa(value control[]) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            "CREATE FUNCTION fp(value record) RETURNS text LANGUAGE plpgsql AS 'BEGIN RETURN NULL; END'",
            "CREATE FUNCTION fp(value anyelement) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        ],
    )?;
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
    Ok(())
}

#[test]
fn public_array_parameter_domains_do_not_select_an_untyped_prepare_signature()
-> testresult::TestResult {
    let analyzed = heavy::analyze("SELECT ARRAY[$1,1]", &[])?;
    assert!(analyzed.context().problems().is_empty());
    let constraints = analyzed.context().parameter_constraints();
    for ty in [PgBuiltInType::int4(), PgBuiltInType::int8()] {
        assert!(constraints.parameter(1).unwrap().constraint().allows(&ty));
        assert!(constraints.allows(&BTreeMap::from([(1, ty.clone())])));
        assert!(
            constraints
                .result_column(0)
                .unwrap()
                .allows(&ty.array_type())
        );
    }
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .is_none()
    );
    let narrowed = heavy::analyze("SELECT ARRAY[$1::integer,1]", &[])?;
    let query = narrowed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4().array_type())
    );
    Ok(())
}

#[rstest::rstest]
#[case::scalar("g(pg_control_system(), NULL)")]
#[case::array("ga(ARRAY[pg_control_system()], NULL)")]
fn record_overloads_preserve_postgres_ambiguity(
    #[case] expression: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(
        &format!("SELECT {expression}"),
        &[
            "CREATE FUNCTION g(value record, other integer) RETURNS integer LANGUAGE plpgsql AS 'BEGIN RETURN 1; END'",
            "CREATE FUNCTION g(value record, other bigint) RETURNS bigint LANGUAGE plpgsql AS 'BEGIN RETURN 1; END'",
            "CREATE FUNCTION ga(value anyarray, other integer) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            "CREATE FUNCTION ga(value anyarray, other bigint) RETURNS bigint LANGUAGE sql AS 'SELECT 1::bigint'",
        ],
    )?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "function_overload_resolution_failed"),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[test]
fn declared_composite_array_fields_use_shipped_catalog_types() -> testresult::TestResult {
    // The shipped catalog and schema-file loader are part of this regression:
    // SQL integer aliases must retain canonical int4 array identity after DDL.
    let analyzed = heavy::analyze(
        "SELECT (NULL::public.array_value).items, ((NULL::public.array_value).items)[1]",
        &["CREATE TYPE public.array_value AS (items integer[])"],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query.result_columns().get_column_at(0).unwrap().col_type(),
        &Some(PgBuiltInType::int4().array_type())
    );
    assert_eq!(
        query.result_columns().get_column_at(1).unwrap().col_type(),
        &Some(PgBuiltInType::int4())
    );
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            AstAndContextPair, PostgresDialect, SqlFunArgs,
            ast::Dialect,
            parse_statements,
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext},
        };
        use std::path::{Path, PathBuf};

        /// Loads shipped builtin JSON metadata and optional SQL definitions per call.
        /// Initialization repeats for each test process; no cache is shared.
        pub(crate) fn analyze(
            sql: &str,
            definitions: &[&str],
        ) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                "17",
            ])?;
            let selected = args.load_metadata()?.remove("default").unwrap();
            let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
            if !definitions.is_empty() {
                let file = ::tempfile::NamedTempFile::new()?;
                std::fs::write(file.path(), definitions.join(";"))?;
                let schema = PostgresDialect::parse_schema_file(
                    file.path(),
                    &base,
                    &mut None,
                    &args,
                    &selected,
                )?;
                assert!(schema.problems().is_empty(), "{:?}", schema.problems());
                base = schema.base_context().clone();
            }
            let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
            Ok(parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context)?)
        }
    }
}
