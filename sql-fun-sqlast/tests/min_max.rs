//! PostgreSQL common-type semantics through the maintained public analysis API.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{ParseContext, PgBuiltInType, TypeInfoRead, TypeReference};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::single("greatest(1)", "int4", Some(true))]
#[case::numeric("greatest(1,2.5)", "numeric", Some(true))]
#[case::numeric_reverse("least(2.5,1)", "numeric", Some(true))]
#[case::null_first("greatest(NULL,1)", "int4", Some(true))]
#[case::null_last("least(1,NULL)", "int4", Some(true))]
#[case::all_null("greatest(NULL,NULL)", "text", Some(false))]
#[case::order_left("greatest('a'::varchar,'b'::bpchar)", "varchar", Some(true))]
#[case::order_right("least('a'::bpchar,'b'::varchar)", "bpchar", Some(true))]
#[case::strings("least('a','b')", "text", Some(true))]
#[case::unknown_string("greatest('2',1)", "int4", Some(true))]
#[case::typed_null("least(NULL::integer,NULL)", "int4", Some(false))]
#[case::preferred("greatest(1::numeric,2::float8,3::int4)", "float8", Some(true))]
#[case::range("least(NULL::int4range,NULL::int4range)", "int4range", Some(false))]
#[case::multirange(
    "greatest(NULL::int4multirange,NULL::int4multirange)",
    "int4multirange",
    Some(false)
)]
#[case::vector(
    "greatest(NULL::int2vector,NULL::int2vector)",
    "int2vector",
    Some(false)
)]
#[case::arrays("greatest('{1}'::int4[],'{2}'::int8[])", "_int8", Some(true))]
#[case::nested("least(greatest(1,NULL),2.5)", "numeric", Some(true))]
fn result_metadata_matches_postgres(
    #[case] expression: &str,
    #[case] ty: &str,
    #[case] not_null: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(&format!("select {expression} as result"), &[])?;
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
    assert_eq!(column.is_not_null(), &not_null);
    let operation = column.expression().unwrap().as_min_max().unwrap();
    assert_eq!(operation.operation(), expression.split('(').next().unwrap());
    assert!(!operation.types().is_any());
    assert_ne!(operation.arguments(), []);
    Ok(())
}

#[rstest::rstest]
#[case::types("greatest(1,true)", "min_max_incompatible_types")]
#[case::typed_string("least('2'::text,1)", "min_max_incompatible_types")]
#[case::preferred_category(r#"least('a'::text,'b'::"char")"#, "min_max_incompatible_types")]
#[case::json("greatest('{}'::json,'{}'::json)", "min_max_not_ordered")]
#[case::json_array("least('{}'::json[],'{}'::json[])", "min_max_not_ordered")]
fn invalid_operands_report_semantic_problems(
    #[case] expression: &str,
    #[case] code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(&format!("select {expression}"), &[])?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{:?}",
        analyzed.context().problems()
    );
    Ok(())
}

#[rstest::rstest]
#[case::domain("greatest(NULL::amount,NULL::amount)", "public.amount")]
#[case::array_domain("greatest(NULL::amount_array,NULL::integer[])", "pg_catalog.int4[]")]
#[case::array_domain_nested(
    "greatest(NULL::nested_amount_array,NULL::integer[])",
    "pg_catalog.int4[]"
)]
#[case::domain_base("least(NULL::amount,1)", "pg_catalog.int4")]
#[case::domain_array("greatest(NULL::amount[],NULL::integer[])", "pg_catalog.int4[]")]
#[case::domain_unknown("greatest(NULL::amount,NULL)", "pg_catalog.int4")]
#[case::enum_values("least('low'::mood,'high'::mood)", "public.mood")]
#[case::composite("greatest(NULL::pair,NULL::pair)", "public.pair")]
fn schema_types_preserve_common_type_rules(
    #[case] expression: &str,
    #[case] ty: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(
        &format!("select {expression} as result"),
        &[
            "create domain amount as integer",
            "create domain amount_array as integer[]",
            "create domain nested_amount_array as amount_array",
            "create type mood as enum ('low','high')",
            "create type pair as (n integer)",
        ],
    )?;
    assert!(
        !analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code().starts_with("min_max_")),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type()
            .as_ref()
            .unwrap()
            .to_string(),
        ty
    );
    Ok(())
}

#[rstest::rstest]
#[case::numeric("select greatest($1,1) as result")]
#[case::nested("select least(greatest($1,1),2) as result")]
#[case::repeated("select greatest($1,$1,1) as result")]
#[case::subquery("select greatest((select $1),1) as result")]
#[case::cte("with c as (select greatest($1,1) as n) select least(c.n,2) as result from c")]
fn parameter_sources_allow_numeric_promotion(
    #[case] sql: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(sql, &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    for ty in [
        PgBuiltInType::int2(),
        PgBuiltInType::int4(),
        PgBuiltInType::int8(),
    ] {
        assert!(constraints.allows(&BTreeMap::from([(1, ty)])));
    }
    assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::bool())])));
    assert!(!constraints.result_column(0).unwrap().is_any());
    Ok(())
}

#[test]
fn all_unknown_parameters_resolve_to_text_and_keep_minmax() -> Result<(), Box<dyn std::error::Error>>
{
    let analyzed = heavy::analyze("select greatest($1,$2) as result", &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let minmax = expression.as_min_max().unwrap();
    assert_eq!(
        minmax.types().admissible_types(),
        Some([PgBuiltInType::text()].as_slice())
    );
    assert_eq!(minmax.arguments().len(), 2);
    assert_eq!(expression.get_type(), Some(PgBuiltInType::text()));
    Ok(())
}

#[test]
fn later_operands_retain_column_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(
        "select greatest(1,t.n) as result from t",
        &["create table t (n integer not null)"],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let value = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_min_max()
        .unwrap();
    assert!(value.arguments()[1].table_column_reference().is_some());
    Ok(())
}

#[test]
fn later_operands_are_resolved_even_after_non_null_literal()
-> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze("select least(1,missing.n)", &[])?;
    assert!(!analyzed.context().problems().is_empty());
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let value = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
        .as_min_max()
        .unwrap();
    assert_eq!(value.arguments().len(), 2);
    assert!(value.arguments()[1].table_column_reference().is_none());
    let binding = ::serde_json::to_value(&value.arguments()[1])?;
    assert!(binding["ColumnRef"]["UndefinedAlias"].is_object());
    Ok(())
}

#[test]
fn incompatible_parameter_uses_report_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze("select greatest($1,1) where $1", &[])?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "parameter_constraint")
    );
    Ok(())
}

#[test]
fn an_explicit_parameter_cast_sets_the_result_type() -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze("select least($1::bigint,1) as result", &[])?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    assert_eq!(expression.get_type(), Some(PgBuiltInType::int8()));
    assert_eq!(expression.as_min_max().unwrap().arguments().len(), 2);
    Ok(())
}

#[test]
fn nullable_operands_do_not_prove_non_null() -> Result<(), Box<dyn std::error::Error>> {
    let analyzed = heavy::analyze(
        "select least(NULL,t.n) as result from t",
        &["create table t(n integer)"],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    assert_eq!(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .is_not_null(),
        &Some(false)
    );
    Ok(())
}

#[rstest::rstest]
fn dynamic_composite_operands_retain_their_type(
    #[values("greatest", "least")] operation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // The shipped function catalog supplies pg_control_system's dynamic record type.
    let analyzed = heavy::analyze(
        &format!("select {operation}(pg_control_system(), pg_control_system()) as result"),
        &[],
    )?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert!(column.col_type().as_ref().unwrap().is_dynamic());
    let expression = column.expression().unwrap().as_min_max().unwrap();
    assert_eq!(expression.operation(), operation);
    assert_eq!(expression.arguments().len(), 2);
    assert_eq!(
        expression.arguments()[0].get_type().as_ref(),
        column.col_type().as_ref()
    );
    Ok(())
}

#[rstest::rstest]
#[case::ordered("int4", Some(true))]
#[case::not_ordered("json", Some(false))]
#[case::unknown_identity("not_a_catalog_type", None)]
fn comparison_catalog_distinguishes_absence_from_negative_answers(
    #[case] name: &str,
    #[case] expected: Option<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Use the shipped catalog to verify real positive/negative facts and context forwarding.
    let analyzed = heavy::analyze("select 1", &[])?;
    let reference = TypeReference::from_master_type_name(name);
    assert_eq!(
        analyzed
            .context()
            .builtin_type_ordered(reference.full_name()),
        expected
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

        /// Parses the full builtin catalog per call to exercise metadata/context forwarding.
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
