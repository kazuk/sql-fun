//! Domain coercions retain nominal results through the public analysis boundary.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    context::ParseContext,
    sem::{PgBuiltInType, TypeReference},
};
use std::collections::BTreeMap;

#[rstest::rstest]
#[case::identity(
    "1::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::smallint(
    "1::smallint::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::bigint(
    "1::bigint::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::string_io(
    "1::bigint::information_schema.character_data",
    "information_schema.character_data"
)]
#[case::identifier(
    "1::smallint::information_schema.sql_identifier",
    "information_schema.sql_identifier"
)]
#[case::timestamp(
    "NULL::timestamptz::information_schema.time_stamp",
    "information_schema.time_stamp"
)]
#[case::null(
    "NULL::information_schema.cardinal_number",
    "information_schema.cardinal_number"
)]
#[case::source_domain("1::public.amount::bigint", "pg_catalog.int8")]
#[case::same_domain("1::public.amount::public.amount", "public.amount")]
#[case::nested("1::public.nested_amount", "public.nested_amount")]
#[case::empty_array("ARRAY[]::public.amounts", "public.amounts")]
#[case::array_domain("ARRAY[1]::public.amounts", "public.amounts")]
#[case::domain_elements("ARRAY[1]::public.amount[]", "public.amount[]")]
fn explicit_cast_retains_domain_binding(
    #[values("17", "18")] version: &str,
    #[case] expression: &str,
    #[case] target: &str,
) -> testresult::TestResult {
    // Real builtin conversions, including I/O casts, are the contract here.
    let analyzed = heavy::analyze(version, &format!("SELECT {expression}"))?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    let expected: TypeReference = target.parse()?;
    let encoded = ::serde_json::to_value(column.expression().unwrap())?;
    assert_eq!(
        encoded["TypeCast"]["target_type"],
        ::serde_json::to_value(expected)?
    );
    assert!(!encoded["TypeCast"]["arg"].is_null());
    assert_eq!(
        encoded["TypeCast"]["resolution"]["Type"],
        encoded["TypeCast"]["target_type"]
    );
    Ok(())
}

#[rstest::rstest]
#[case::invalid("true::information_schema.time_stamp", "explicit_cast_not_defined")]
#[case::invalid_array(
    "NULL::boolean[]::information_schema.time_stamp[]",
    "explicit_cast_not_defined"
)]
#[case::missing("1::public.no_such_domain", "parameter_constraint")]
fn invalid_cast_keeps_target_lookup_and_argument(
    #[values("17", "18")] version: &str,
    #[case] expression: &str,
    #[case] code: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(version, &format!("SELECT {expression}"))?;
    assert!(
        analyzed
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == code),
        "{:?}",
        analyzed.context().problems()
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let encoded = ::serde_json::to_value(
        query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap(),
    )?;
    assert!(!encoded["TypeCast"]["arg"].is_null());
    if expression.contains("no_such_domain") {
        assert_eq!(encoded["TypeCast"]["resolution"], "NotFound");
    } else {
        assert_eq!(
            encoded["TypeCast"]["resolution"]["Type"],
            encoded["TypeCast"]["target_type"]
        );
    }
    Ok(())
}

#[rstest::rstest]
fn parameter_cast_preserves_admissible_inputs_and_domain_output(
    #[values("17", "18")] version: &str,
) -> testresult::TestResult {
    let analyzed = heavy::analyze(version, "SELECT $1::public.nested_amount")?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int8())])));
    assert!(!constraints.allows(&BTreeMap::from([(
        1,
        TypeReference::from_master_type_name("date")
    )])));
    let target: TypeReference = "public.nested_amount".parse()?;
    assert!(constraints.result_column(0).unwrap().allows(&target));
    assert!(constraints.allows(&BTreeMap::from([(1, "public.amount".parse()?)])));
    assert!(
        !constraints
            .result_column(0)
            .unwrap()
            .allows(&PgBuiltInType::int4())
    );
    Ok(())
}

// Removed shipped_postgres18_catalog_domain_casts_resolve; restore from 81568731f391becc60f90cbeac057ddd96c0baa6.

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_core::SqlFunArgs;
        use ::sql_fun_sqlast::{
            AstAndContextPair,
            ast::Dialect,
            context::{BaseContext, FrozenBaseContext, ParseContext, SchemaFileContext},
            parse_statements,
        };
        use std::path::{Path, PathBuf};

        /// Load real builtin cast metadata and small domain DDL per test process.
        /// No shared setup.
        pub(crate) fn analyze(
            version: &str,
            sql: &str,
        ) -> Result<AstAndContextPair<SchemaFileContext>, Box<dyn std::error::Error>> {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                version,
            ])?;
            let selected = args.load_metadata()?.remove("default").unwrap();
            let mut base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
            let mut context =
                SchemaFileContext::new(Path::new("domains.sql"), &base, &args, &selected);
            // Minimal real DDL; do not parse the whole catalog for small cases.
            for statement in parse_statements(
                r"
                    CREATE SCHEMA information_schema;
                    CREATE DOMAIN information_schema.cardinal_number AS integer;
                    CREATE DOMAIN information_schema.character_data AS varchar;
                    CREATE DOMAIN information_schema.sql_identifier AS name;
                    CREATE DOMAIN information_schema.time_stamp AS timestamptz;
                    CREATE DOMAIN public.amount AS integer;
                    CREATE DOMAIN public.nested_amount AS public.amount;
                    CREATE DOMAIN public.amounts AS integer[];
                ",
                Dialect::postgres(),
            ) {
                context = statement?.analyze(context)?.into_context();
            }
            assert!(context.problems().is_empty(), "{:?}", context.problems());
            base = context.freeze();
            let context = SchemaFileContext::new(Path::new("query.sql"), &base, &args, &selected);
            Ok(parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()?
                .analyze(context)?)
        }
    }
}
