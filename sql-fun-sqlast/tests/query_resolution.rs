//! Compatibility checks against the public, non-unit-test analysis boundary.

use self::test_helpers::heavy;

#[test]
fn supported_select_reaches_public_resolution() {
    assert!(heavy::analyze("select 1 as n").is_ok());
}

#[rstest::rstest]
#[case("select row(1, 2)")]
#[case("select true is true")]
#[case("select json_object('x': 1)")]
#[case("select 'x' collate \"C\"")]
#[case("select merge_action()")]
#[case("select (1).*")]
#[case("select 1 from coalesce(1, 2) as t")]
#[case("select 1 from rows from(coalesce(1, 2)) as t")]
#[case("select x from xmltable('/x' passing '<x/>' columns x text path '.') as t")]
#[case("select x from json_table('{}', '$' columns(x text path '$.x')) as j")]
#[case(
    "with c as (merge into missing using other on true when matched then do nothing returning merge_action()) select 1"
)]
fn unsupported_raw_syntax_retains_analysis_error(#[case] sql: &str) {
    assert!(heavy::analyze(sql).is_err(), "{sql}");
}

#[test]
#[should_panic(expected = "not yet implemented")]
fn table_sample_retains_explicit_unimplemented_marker() {
    let _ = heavy::analyze("select 1 from missing tablesample system(1)");
}

#[test]
fn grouping_reaches_the_public_semantic_boundary() {
    // The production catalog verifies bindings at the public analysis boundary.
    let analysis = heavy::query(
        "SELECT GROUPING(a) AS mask FROM (SELECT 1 AS a) s GROUP BY ROLLUP(a) ORDER BY GROUPING(a)",
    )
    .unwrap();
    assert!(analysis.context().problems().is_empty());
    let query = analysis.ast().clone().into_select_statemtnt().unwrap();
    let expression = query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap();
    let grouping = expression.as_grouping().unwrap();
    assert!(grouping.is_valid());
    assert_eq!(grouping.arguments()[0].key_index(), Some(0));
    assert_eq!(
        expression.get_type(),
        Some(::sql_fun_sqlast::sem::PgBuiltInType::int4())
    );
    assert_eq!(expression.is_not_null(), Some(true));
    assert!(
        query.query_clauses().unwrap().order()[0]
            .expression()
            .as_grouping()
            .unwrap()
            .is_valid()
    );
}

#[test]
fn builtin_aggregate_rejects_nested_grouping() {
    // Real builtin metadata, through SchemaFileContext and its frozen base.
    let analysis =
        heavy::query("SELECT sum(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a").unwrap();
    assert!(
        analysis
            .context()
            .problems()
            .iter()
            .any(|problem| problem.code() == "query_invalid_grouping_placement")
    );
}

mod test_helpers {
    pub(super) mod heavy {
        use std::path::PathBuf;

        use ::sql_fun_sqlast::{
            PostgresDialect, SqlFunArgs,
            sem::{
                AnalysisError, BaseContext, FrozenBaseContext, SchemaFileContext, SelectStatement,
            },
            syn::{ListOpt, Opt},
        };

        /// Loads builtin JSON catalogs for each invocation. These integration checks
        /// require the production catalog and public analysis boundary; no cache is shared.
        pub(crate) fn query(
            sql: &str,
        ) -> Result<::sql_fun_sqlast::AstAndContextPair<SchemaFileContext>, AnalysisError> {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().expect("metadata path"),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                "17",
            ])
            .expect("fixture arguments");
            let collection = args.load_metadata().expect("fixture metadata");
            let selected = collection.get("default").expect("selected test metadata");
            let base = BaseContext::new(&args, selected).expect("builtin catalog fixture");
            let context = {
                let analysis_base = &FrozenBaseContext::from(base);
                SchemaFileContext::new(&metadata, analysis_base, &args, selected)
            };
            let syntax = PostgresDialect::parse(sql).expect("legal SELECT syntax");
            let tokens = PostgresDialect::scan(sql).expect("SELECT tokens");
            let select = syntax
                .get_stmts()
                .get(0)
                .get_stmt()
                .as_select_stmt()
                .as_inner()
                .expect("SELECT fixture");
            SelectStatement::analyze_select(context, select, &tokens)
        }

        /// Loads the real builtin catalogs on every invocation, as query does.
        pub(crate) fn analyze(sql: &str) -> Result<(), AnalysisError> {
            query(sql).map(|_| ())
        }
    }
}
