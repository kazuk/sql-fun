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
fn exists_retains_non_null_boolean_and_analyzed_child_at_public_boundary() {
    // The real builtin catalog verifies public name resolution and inference.
    let analyzed =
        heavy::analyze("SELECT EXISTS (SELECT $1::integer AS n WHERE $1::integer < 2) AS found")
            .expect("EXISTS analysis");
    let ::sql_fun_sqlast::sem::SemAst::SelectStatement(query) = analyzed else {
        panic!("SELECT semantics expected");
    };
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(
        column.col_type(),
        &Some(::sql_fun_sqlast::sem::PgBuiltInType::bool())
    );
    assert_eq!(column.is_not_null(), &Some(true));
    let exists = column
        .expression()
        .unwrap()
        .as_exists()
        .expect("EXISTS retained after inference");
    assert_eq!(exists.query().result_columns().len(), 1);
    assert_eq!(
        exists
            .query()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .col_type(),
        &Some(::sql_fun_sqlast::sem::PgBuiltInType::int4()),
    );
}

mod test_helpers {
    pub(super) mod heavy {
        use std::path::PathBuf;

        use ::sql_fun_sqlast::{
            PostgresDialect, SqlFunArgs,
            sem::{
                AnalysisError, BaseContext, FrozenBaseContext, SchemaFileContext, SelectStatement,
                SemAst,
            },
            syn::{ListOpt, Opt},
        };

        /// Loads builtin JSON catalogs for each invocation to check the public
        /// analysis boundary. Initialization is repeated; no cache is shared.
        pub(crate) fn analyze(sql: &str) -> Result<SemAst, AnalysisError> {
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
                .map(|analyzed| analyzed.ast().clone())
        }
    }
}
