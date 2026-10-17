//! DELETE analysis through the public catalog and statement APIs.
use self::test_helpers::heavy;
use ::sql_fun_sqlast::{
    ast::Dialect,
    parse_statements,
    sem::{BaseParseContext, ColumnName, FullName, ParseContext, PgBuiltInType, TableName},
};

#[test]
fn real_catalog_delete_cte_preserves_bindings_types_and_schema()
-> Result<(), Box<dyn std::error::Error>> {
    // The full builtin catalog is required here to exercise PostgreSQL's real
    // equality-operator signatures and the constraints derived from them.
    let mut context = heavy::context()?;
    for statement in parse_statements(
        "CREATE TABLE items(id integer NOT NULL, label text); CREATE TABLE sources(id integer, label text)",
        Dialect::postgres(),
    ) {
        context = statement?.analyze(context)?.into_context();
    }
    let statement = parse_statements("WITH d(removed,label) AS (DELETE FROM items AS i USING sources AS s WHERE i.id=s.id AND i.id=$1 RETURNING i.id,s.label) SELECT removed,label FROM d", Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(context)?;
    assert!(
        analyzed.context().problems().is_empty(),
        "{:?}",
        analyzed.context().problems()
    );
    let constraints = analyzed.context().parameter_constraints();
    assert!(
        constraints
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int4())
    );
    assert!(
        !constraints
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::bool())
    );
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    let column = query.result_columns().get_column_at(0).unwrap();
    assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
    assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), &Some(true));
    assert!(
        analyzed
            .context()
            .get_table(&TableName::from(FullName::with_schema("public", "items")))
            .is_some()
    );
    Ok(())
}

mod test_helpers {
    pub(super) mod heavy {
        use ::sql_fun_sqlast::{
            SqlFunArgs,
            sem::{
                BaseContext, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext,
                SourcePathWithSpan,
            },
        };
        use std::path::Path;

        /// Parses the full builtin catalog once per call to exercise real overloads.
        pub(crate) fn context() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
            let metadata = Path::new(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
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
            let base = BaseContext::new(&args, &selected)?;
            let mut context = SchemaFileContext::new(
                Path::new("delete.sql"),
                &FrozenBaseContext::from(base),
                &args,
                &selected,
            );
            context.set_source_path_with_span(&SourcePathWithSpan::new(
                "delete.sql",
                &::sql_fun_sqlast::StringSpan::new(0, 0),
            ));
            Ok(context)
        }
    }
}
