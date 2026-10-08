//! Compatibility checks against the public, non-unit-test analysis boundary.

use self::test_helpers::analyze;

#[test]
fn supported_select_reaches_public_resolution() {
    assert!(analyze("select 1 as n").is_ok());
}

#[rstest::rstest]
#[case("select row(1, 2)")]
#[case("select true is true")]
#[case("select grouping(1)")]
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
    assert!(analyze(sql).is_err(), "{sql}");
}

#[test]
#[should_panic(expected = "not yet implemented")]
fn table_sample_retains_explicit_unimplemented_marker() {
    let _ = analyze("select 1 from missing tablesample system(1)");
}

mod test_helpers {
    use std::path::PathBuf;

    use ::sql_fun_sqlast::{
        PostgresDialect, SqlFunArgs,
        sem::{AnalysisError, BaseContext, FrozenBaseContext, SchemaFileContext, SelectStatement},
        syn::{ListOpt, Opt},
    };

    pub(super) fn analyze(sql: &str) -> Result<(), AnalysisError> {
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
        let base = BaseContext::new(
            &args,
            args.load_metadata()
                .expect("fixture metadata")
                .get("default")
                .expect("selected test metadata"),
        )
        .expect("builtin catalog fixture");
        let context = SchemaFileContext::new(&metadata, &FrozenBaseContext::from(base));
        let syntax = PostgresDialect::parse(sql).expect("legal SELECT syntax");
        let tokens = PostgresDialect::scan(sql).expect("SELECT tokens");
        let select = syntax
            .get_stmts()
            .get(0)
            .get_stmt()
            .as_select_stmt()
            .as_inner()
            .expect("SELECT fixture");
        SelectStatement::analyze_select(context, select, &tokens).map(|_| ())
    }
}
