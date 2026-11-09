use crate::extract::{
    definition_schemas::DefinitionSchemas,
    render::{Renderer, fragment, identifier},
};

use std::{collections::BTreeMap, fmt::Write, path::Path, time::Duration};

use ::serde_json::Value;
use ::sql_fun_core::PostgresVersion;
use ::tokio::io::AsyncWriteExt;

async fn run_psql(configuration: &str, sql: &str) -> Result<String, anyhow::Error> {
    let mut child = tokio::process::Command::new("psql")
        .args([
            "-X",
            "--quiet",
            "--no-align",
            "--tuples-only",
            "--record-separator-zero",
            "--no-password",
            "--set=ON_ERROR_STOP=1",
        ])
        .arg("--dbname")
        .arg(configuration)
        .env("PGCLIENTENCODING", "UTF8")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            anyhow::anyhow!("could not start psql; install the PostgreSQL client tools")
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("psql stdin unavailable"))?;
    // Drain output while writing so large queries/results cannot deadlock the pipes.
    let write = async {
        stdin.write_all(sql.as_bytes()).await?;
        drop(stdin);
        Ok::<(), std::io::Error>(())
    };
    let (written, output) = tokio::join!(write, child.wait_with_output());
    written?;
    let output = output?;
    anyhow::ensure!(
        output.status.success(),
        "psql extraction failed; check connection, libpq TLS configuration, server version, and catalog permissions (details redacted)"
    );
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) async fn wait_ready(configuration: &str) -> Result<(), anyhow::Error> {
    loop {
        if let Ok(Ok(_)) = tokio::time::timeout(
            Duration::from_secs(2),
            run_psql(configuration, "SELECT 1;\n"),
        )
        .await
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn next_catalog(lines: &mut std::str::SplitTerminator<'_, char>) -> Result<Value, anyhow::Error> {
    let mut value: Value = serde_json::from_str(
        lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing psql catalog result"))?,
    )?;
    value.sort_all_objects();
    Ok(value)
}

#[cfg(test)]
mod test_next_catalog {
    use crate::extract::catalog::next_catalog;

    #[test]
    fn consumes_one_nul_delimited_result_and_sorts_nested_objects() {
        let output = concat!(
            r#"{"z":{"b":2,"a":1},"a":[{"d":4,"c":3}]}"#,
            "\0",
            "[]",
            "\0"
        );
        let mut lines = output.split_terminator('\0');
        let value = next_catalog(&mut lines).unwrap();
        assert_eq!(
            value.to_string(),
            r#"{"a":[{"c":3,"d":4}],"z":{"a":1,"b":2}}"#
        );
        assert_eq!(next_catalog(&mut lines).unwrap(), serde_json::json!([]));
        assert_eq!(
            next_catalog(&mut lines).unwrap_err().to_string(),
            "missing psql catalog result"
        );
    }

    #[rstest::rstest]
    #[case::empty_record("\0")]
    #[case::malformed("{\0")]
    #[case::multiple_values("[] []\0")]
    fn rejects_invalid_json(#[case] output: &str) {
        assert!(next_catalog(&mut output.split_terminator('\0')).is_err());
    }
}

pub(crate) async fn detect(
    configuration: &str,
) -> Result<sql_fun_core::PostgresVersion, anyhow::Error> {
    let output = run_psql(
        configuration,
        "SHOW server_version_num; SHOW server_version;\n",
    )
    .await?;
    parse_identity(&output)
}

fn parse_identity(output: &str) -> Result<PostgresVersion, anyhow::Error> {
    let mut values = output.split_terminator('\0');
    let number = values
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing server version"))?
        .parse()?;
    let text = values
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing server version identity"))?;
    sql_fun_core::PostgresVersionInventory::builtin()
        .detect(number, text)
        .map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod test_parse_identity {
    use crate::extract::catalog::parse_identity;

    #[test]
    fn parses_nul_delimited_server_identity() {
        let identity = parse_identity(concat!("170002\0", "17.2 (packaged build)\0")).unwrap();
        assert_eq!(identity.major(), 17);
        assert!(identity.is_stable());
    }

    #[rstest::rstest]
    #[case::missing("", "missing server version")]
    #[case::missing_identity("170002\0", "missing server version identity")]
    fn rejects_missing_records(#[case] output: &str, #[case] expected: &str) {
        assert_eq!(parse_identity(output).unwrap_err().to_string(), expected);
    }

    #[rstest::rstest]
    #[case::non_numeric(concat!("version\0", "17.2\0"))]
    #[case::overflow(concat!("9999999999999999\0", "17.2\0"))]
    #[case::inconsistent(concat!("170000\0", "18beta1\0"))]
    fn rejects_invalid_versions(#[case] output: &str) {
        assert!(parse_identity(output).is_err());
    }
}

pub(super) async fn extract(
    configuration: &str,
    output: &Path,
    provenance: &Value,
    schemas: &DefinitionSchemas,
) -> Result<(), anyhow::Error> {
    let identity = detect(configuration).await?;
    let sql = extraction_sql(schemas)?;
    let results = run_psql(configuration, &sql).await?;
    write_catalogs(&results, &identity, output, provenance, schemas)
}

fn extraction_sql(schemas: &DefinitionSchemas) -> Result<String, anyhow::Error> {
    let queries = [
        include_str!("sql/builtin_types.sql"),
        include_str!("sql/builtin_funcs.sql"),
        include_str!("sql/builtin_func_detail.sql"),
        include_str!("sql/builtin_binary_ops.sql"),
        include_str!("sql/builtin_left_unary_ops.sql"),
        include_str!("sql/builtin_right_unary_ops.sql"),
        include_str!("sql/builtin_casts.sql"),
        include_str!("sql/domains.sql"),
        include_str!("sql/builtin_columns.sql"),
        include_str!("sql/schema_functions.sql"),
        include_str!("sql/views.sql"),
        include_str!("sql/collations.sql"),
        include_str!("sql/operator_classes.sql"),
    ];
    let mut sql = String::from(
        "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\nSET LOCAL search_path = pg_catalog; SET LOCAL standard_conforming_strings = on; SET LOCAL extra_float_digits = 3; SET LOCAL timezone = 'UTC'; SET LOCAL statement_timeout = '60s';\nSHOW server_version_num;\n",
    );
    sql.push_str(&schemas.session_sql()?);
    for query in queries {
        writeln!(
            sql,
            "SELECT ({})::text;",
            query.trim().trim_end_matches(';')
        )?;
    }
    sql.push_str("COMMIT;\n");
    Ok(sql)
}

#[cfg(test)]
mod test_extraction_sql {
    use crate::extract::catalog::extraction_sql;
    use crate::extract::definition_schemas::DefinitionSchemas;

    #[test]
    fn builds_parseable_read_only_transaction_with_explicit_schema_selection() {
        let schemas = DefinitionSchemas::new(&["odd' schema".to_owned()]).unwrap();
        let sql = extraction_sql(&schemas).unwrap();
        assert!(sql.starts_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\n"));
        assert!(sql.ends_with("COMMIT;\n"));
        assert!(sql.contains(&schemas.session_sql().unwrap()));
        pg_query::parse(&sql, ::pg_query::ParserOptions::default()).unwrap();
    }
}

fn write_catalogs(
    results: &str,
    identity: &PostgresVersion,
    output: &Path,
    provenance: &Value,
    schemas: &DefinitionSchemas,
) -> Result<(), anyhow::Error> {
    let mut lines = results.split_terminator('\0');
    let version: u32 = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing server version"))?
        .parse()?;
    anyhow::ensure!(
        version / 10_000 == identity.major(),
        "server version changed during extraction"
    );
    anyhow::ensure!(
        next_catalog(&mut lines)?.as_bool() == Some(true),
        "selected definition schema does not exist"
    );
    for filename in [
        "pg_type.json",
        "functions.json",
        "func_detail.json",
        "binary_op.json",
        "left_unary_op.json",
        "right_unary_op.json",
        "casts.json",
    ] {
        write_json(&output.join(filename), &next_catalog(&mut lines)?)?;
    }
    std::fs::write(output.join("schema.sql"), render_schema(&mut lines)?)?;
    for filename in ["collations.json", "operator_classes.json"] {
        write_json(&output.join(filename), &next_catalog(&mut lines)?)?;
    }
    anyhow::ensure!(lines.next().is_none(), "unexpected psql catalog result");
    let mut metadata = serde_json::json!({
        "format_version": 1, "server_version_num": version,
        "provenance": provenance,
        "scope": "bootstrap-pg_catalog-and-information_schema"
    });
    schemas.record_scope(&mut metadata);
    if !identity.is_stable() {
        metadata["version_identity"] = identity.to_string().into();
    }
    write_json(&output.join("metadata.json"), &metadata)?;
    Ok(())
}

#[cfg(test)]
mod test_write_catalogs {
    use crate::extract::catalog::write_catalogs;
    use crate::extract::definition_schemas::DefinitionSchemas;
    use crate::test_helpers::{catalog_results, extraction_results};
    use ::sql_fun_core::PostgresVersion;

    #[test]
    fn maps_results_to_artifacts_and_records_provenance_and_selected_scope() {
        let payloads = [
            ("pg_type.json", "types"),
            ("functions.json", "functions"),
            ("func_detail.json", "function details"),
            ("binary_op.json", "binary"),
            ("left_unary_op.json", "left"),
            ("right_unary_op.json", "right"),
            ("casts.json", "casts"),
            ("collations.json", "collations"),
            ("operator_classes.json", "operator classes"),
        ];
        let mut catalogs = Vec::new();
        for (_, marker) in &payloads[..7] {
            catalogs.push(serde_json::json!({"catalog":marker}));
        }
        catalogs.extend(vec![serde_json::json!([]); 4]);
        for (_, marker) in &payloads[7..] {
            catalogs.push(serde_json::json!({"catalog":marker}));
        }
        let results = extraction_results(170_002, true, &catalogs);
        let directory = tempfile::tempdir().unwrap();
        let provenance = serde_json::json!({"source":"unit test"});
        let schemas = DefinitionSchemas::new(&["z".into(), "a".into()]).unwrap();
        write_catalogs(
            &results,
            &"17".parse().unwrap(),
            directory.path(),
            &provenance,
            &schemas,
        )
        .unwrap();
        for (filename, marker) in payloads {
            let actual: serde_json::Value =
                serde_json::from_slice(&std::fs::read(directory.path().join(filename)).unwrap())
                    .unwrap();
            assert_eq!(actual, serde_json::json!({"catalog":marker}), "{filename}");
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(
            metadata,
            serde_json::json!({
                "format_version":1, "server_version_num":170_002,
                "provenance":provenance, "scope":"bootstrap-pg_catalog-and-information_schema",
                "definition_schemas":["a","z"]
            })
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("schema.sql")).unwrap(),
            "-- PostgreSQL builtin schema; generated by sql-fun extract, format 1.\n"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 11);
    }

    #[rstest::rstest]
    #[case::stable("17", false)]
    #[case::prerelease("17beta1", true)]
    fn records_prerelease_identity_only_when_needed(
        #[case] identity: &str,
        #[case] prerelease: bool,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let results = extraction_results(170_000, true, &vec![serde_json::json!([]); 13]);
        let version: PostgresVersion = identity.parse().unwrap();
        write_catalogs(
            &results,
            &version,
            directory.path(),
            &serde_json::json!({}),
            &DefinitionSchemas::new(&[]).unwrap(),
        )
        .unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("metadata.json")).unwrap())
                .unwrap();
        let expected = if prerelease {
            Some(serde_json::json!(identity))
        } else {
            None
        };
        assert_eq!(metadata.get("version_identity"), expected.as_ref());
        assert!(metadata.get("definition_schemas").is_none());
    }

    #[rstest::rstest]
    #[case::missing_version("", "missing server version")]
    #[case::changed_version("180000\0true\0", "server version changed during extraction")]
    #[case::missing_schema("170000\0false\0", "selected definition schema does not exist")]
    #[case::invalid_schema_result("170000\0[]\0", "selected definition schema does not exist")]
    fn rejects_invalid_preamble(#[case] results: &str, #[case] expected: &str) {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            write_catalogs(
                results,
                &"17".parse().unwrap(),
                directory.path(),
                &serde_json::json!({}),
                &DefinitionSchemas::new(&[]).unwrap()
            )
            .unwrap_err()
            .to_string(),
            expected
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[rstest::rstest]
    fn rejects_each_truncated_catalog(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)] count: usize,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let results = extraction_results(170_000, true, &vec![serde_json::json!([]); count]);
        assert_eq!(
            write_catalogs(
                &results,
                &"17".parse().unwrap(),
                directory.path(),
                &serde_json::json!({}),
                &DefinitionSchemas::new(&[]).unwrap()
            )
            .unwrap_err()
            .to_string(),
            "missing psql catalog result"
        );
        assert!(!directory.path().join("metadata.json").exists());
    }

    #[test]
    fn rejects_surplus_results_before_writing_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let results = extraction_results(170_000, true, &vec![serde_json::json!([]); 14]);
        assert_eq!(
            write_catalogs(
                &results,
                &"17".parse().unwrap(),
                directory.path(),
                &serde_json::json!({}),
                &DefinitionSchemas::new(&[]).unwrap()
            )
            .unwrap_err()
            .to_string(),
            "unexpected psql catalog result"
        );
        assert!(!directory.path().join("metadata.json").exists());
    }

    #[test]
    fn propagates_invalid_json_and_output_errors() {
        let directory = tempfile::tempdir().unwrap();
        let schemas = DefinitionSchemas::new(&[]).unwrap();
        let identity = "17".parse().unwrap();
        let provenance = serde_json::json!({});
        assert!(
            write_catalogs(
                "170000\0true\0{\0",
                &identity,
                directory.path(),
                &provenance,
                &schemas
            )
            .is_err()
        );
        let results = extraction_results(170_000, true, &vec![serde_json::json!([]); 13]);
        let error = write_catalogs(
            &results,
            &identity,
            &directory.path().join("missing"),
            &provenance,
            &schemas,
        )
        .unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        let malformed = catalog_results(&[serde_json::json!("not a version")]);
        assert!(
            write_catalogs(
                &malformed,
                &identity,
                directory.path(),
                &provenance,
                &schemas
            )
            .is_err()
        );
    }
}

fn render_schema(lines: &mut std::str::SplitTerminator<'_, char>) -> Result<String, anyhow::Error> {
    let renderer = Renderer::new()?;
    let mut schema =
        String::from("-- PostgreSQL builtin schema; generated by sql-fun extract, format 1.\n");
    let mut domains = next_catalog(lines)?;
    type_fields(&mut domains, &["schema", "name"], &["definition"])?;
    schema.push_str(&renderer.render("domains", &domains)?);
    let columns = next_catalog(lines)?;
    let mut tables = BTreeMap::<(String, String), Vec<Value>>::new();
    for mut column in columns
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid column catalog"))?
        .clone()
    {
        let schema = required_string(&column, "schema")?.to_owned();
        let table = required_string(&column, "table")?.to_owned();
        type_row(&mut column, &["column"], &["data_type", "default_expr"])?;
        tables.entry((schema, table)).or_default().push(column);
    }
    let mut table_rows = Vec::new();
    for ((schema, table), columns) in tables {
        table_rows.push(serde_json::json!({"schema": identifier(&schema), "table": identifier(&table), "columns": columns}));
    }
    schema.push_str(&renderer.render("tables", &Value::Array(table_rows))?);
    let mut functions = next_catalog(lines)?;
    type_fields(
        &mut functions,
        &["schema", "name"],
        &["arguments", "result"],
    )?;
    schema.push_str(&renderer.render("functions", &functions)?);
    let mut views = next_catalog(lines)?;
    qualify_view_rules(&mut views)?;
    type_fields(
        &mut views,
        &["schema", "view"],
        &["definition", "options", "declarations"],
    )?;
    schema.push_str(&renderer.render("views", &views)?);
    Ok(schema)
}

#[cfg(test)]
mod test_render_schema {
    use crate::extract::catalog::render_schema;
    use crate::test_helpers::catalog_results;

    #[test]
    fn groups_tables_by_schema_and_name_preserving_column_order() {
        let output = catalog_results(&[
            serde_json::json!([]),
            serde_json::json!([
                {"schema":"z", "table":"t", "column":"second", "data_type":"text", "not_null":false, "default_expr":null},
                {"schema":"a", "table":"t", "column":"only", "data_type":"integer", "not_null":true, "default_expr":"1"},
                {"schema":"z", "table":"t", "column":"first", "data_type":"integer", "not_null":false, "default_expr":null}
            ]),
            serde_json::json!([]),
            serde_json::json!([]),
        ]);
        assert_eq!(
            render_schema(&mut output.split_terminator('\0')).unwrap(),
            concat!(
                "-- PostgreSQL builtin schema; generated by sql-fun extract, format 1.\n",
                r#"CREATE TABLE "a"."t" (
    "only" integer NOT NULL DEFAULT (1)
);
CREATE TABLE "z"."t" (
    "second" text,
    "first" integer
);
"#
            )
        );
    }

    #[test]
    fn renders_all_schema_sections_and_leaves_following_catalog_unread() {
        let output = catalog_results(&[
            serde_json::json!([{"schema":r#"odd" schema"#, "name":"D", "definition":"text CHECK (VALUE <> '<&>')"}]),
            serde_json::json!([]),
            serde_json::json!([{"schema":"s", "name":"f", "arguments":"x integer", "result":"text"}]),
            serde_json::json!([{"schema":"s", "view":"v", "definition":"SELECT '<&>'::text AS x;", "options":"WITH (security_barrier=true)", "declarations":"", "rules":[]}]),
            serde_json::json!({"remaining":true}),
        ]);
        let mut lines = output.split_terminator('\0');
        assert_eq!(
            render_schema(&mut lines).unwrap(),
            concat!(
                "-- PostgreSQL builtin schema; generated by sql-fun extract, format 1.\n",
                r#"CREATE DOMAIN "odd"" schema"."D" AS text CHECK (VALUE <> '<&>');
CREATE FUNCTION "s"."f"(x integer) RETURNS text LANGUAGE sql AS '';
CREATE VIEW "s"."v" WITH (security_barrier=true) AS SELECT '<&>'::text AS x;

"#
            )
        );
        assert_eq!(lines.next(), Some(r#"{"remaining":true}"#));
    }

    #[rstest::rstest]
    #[case::non_array(serde_json::json!({}), "invalid column catalog")]
    #[case::missing_schema(serde_json::json!([{"table":"t"}]), "missing catalog field schema")]
    #[case::missing_table(serde_json::json!([{"schema":"s"}]), "missing catalog field table")]
    fn rejects_invalid_columns(#[case] columns: serde_json::Value, #[case] expected: &str) {
        let output = catalog_results(&[serde_json::json!([]), columns]);
        assert_eq!(
            render_schema(&mut output.split_terminator('\0'))
                .unwrap_err()
                .to_string(),
            expected
        );
    }

    #[test]
    fn rejects_truncated_results() {
        assert_eq!(
            render_schema(&mut "[]".split_terminator('\0'))
                .unwrap_err()
                .to_string(),
            "missing psql catalog result"
        );
    }
}

// pg_get_ruledef omits visible target schemas. Bind the known bootstrap
// view explicitly, rejecting an unexpected server deparser prefix rather than
// loading a rule against whichever schema happens to be first in search_path.
fn qualify_view_rules(views: &mut Value) -> Result<(), anyhow::Error> {
    for view in views
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("invalid view catalog rows"))?
    {
        let mut declarations = required_string(view, "declarations")?.to_owned();
        for rule in view["rules"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing view rule catalog"))?
        {
            let definition = required_string(rule, "definition")?;
            let prefix = required_string(rule, "prefix")?;
            let suffix = definition
                .strip_prefix(prefix)
                .ok_or_else(|| anyhow::anyhow!("unexpected PostgreSQL rule deparser prefix"))?;
            if !declarations.is_empty() {
                declarations.push('\n');
            }
            declarations.push_str(required_string(rule, "qualified_prefix")?);
            declarations.push_str(suffix);
        }
        view["declarations"] = Value::String(declarations);
    }
    Ok(())
}

#[cfg(test)]
mod test_view_rule_qualification {
    use crate::extract::catalog::qualify_view_rules;

    #[test]
    fn qualifies_only_the_checked_target_prefix() {
        let mut views = serde_json::json!([{
            "declarations": "",
            "rules": [{
                "prefix": "CREATE RULE r AS\n    ON UPDATE TO v",
                "qualified_prefix": "CREATE RULE r AS\n    ON UPDATE TO pg_catalog.v",
                "definition": "CREATE RULE r AS\n    ON UPDATE TO v DO INSTEAD NOTHING;"
            }]
        }]);
        qualify_view_rules(&mut views).unwrap();
        assert_eq!(
            views[0]["declarations"],
            "CREATE RULE r AS\n    ON UPDATE TO pg_catalog.v DO INSTEAD NOTHING;"
        );
        views[0]["rules"][0]["definition"] = serde_json::json!("unexpected server format");
        assert!(qualify_view_rules(&mut views).is_err());
    }
    #[test]
    fn appends_rules_in_order_after_existing_declarations() {
        let mut views = serde_json::json!([{
            "declarations":"ALTER VIEW s.v OWNER TO owner;",
            "rules":[
                {"prefix":"ON v", "qualified_prefix":"ON s.v", "definition":"ON v DO SELECT 'ON v';"},
                {"prefix":"ON v", "qualified_prefix":"ON s.v", "definition":"ON v DO NOTHING;"}
            ]
        }, {"declarations":"", "rules":[]}]);
        qualify_view_rules(&mut views).unwrap();
        assert_eq!(
            views[0]["declarations"],
            "ALTER VIEW s.v OWNER TO owner;\nON s.v DO SELECT 'ON v';\nON s.v DO NOTHING;"
        );
        assert_eq!(views[1]["declarations"], "");
    }

    #[rstest::rstest]
    #[case::not_array(serde_json::json!({}), "invalid view catalog rows")]
    #[case::declarations(serde_json::json!([{"rules":[]}]), "missing catalog field declarations")]
    #[case::rules(serde_json::json!([{"declarations":""}]), "missing view rule catalog")]
    #[case::definition(serde_json::json!([{"declarations":"", "rules":[{}]}]), "missing catalog field definition")]
    #[case::prefix(serde_json::json!([{"declarations":"", "rules":[{"definition":"rule"}]}]), "missing catalog field prefix")]
    #[case::qualified_prefix(serde_json::json!([{"declarations":"", "rules":[{"definition":"rule", "prefix":"rule"}]}]), "missing catalog field qualified_prefix")]
    fn rejects_incomplete_catalog(#[case] mut views: serde_json::Value, #[case] expected: &str) {
        assert_eq!(
            qualify_view_rules(&mut views).unwrap_err().to_string(),
            expected
        );
    }
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, anyhow::Error> {
    if let Some(value) = value.get(field)
        && let Some(value) = value.as_str()
    {
        return Ok(value);
    }
    Err(anyhow::anyhow!("missing catalog field {field}"))
}

#[cfg(test)]
mod test_required_string {
    use crate::extract::catalog::required_string;

    #[rstest::rstest]
    #[case::text(serde_json::json!({"name":"雪"}), Some("雪"))]
    #[case::empty(serde_json::json!({"name":""}), Some(""))]
    #[case::missing(serde_json::json!({}), None)]
    #[case::null(serde_json::json!({"name":null}), None)]
    #[case::number(serde_json::json!({"name":42}), None)]
    #[case::not_object(serde_json::json!([]), None)]
    fn requires_a_string_field(#[case] value: serde_json::Value, #[case] expected: Option<&str>) {
        match expected {
            Some(expected) => assert_eq!(required_string(&value, "name").unwrap(), expected),
            None => assert_eq!(
                required_string(&value, "name").unwrap_err().to_string(),
                "missing catalog field name"
            ),
        }
    }
}

fn type_fields(
    value: &mut Value,
    identifiers: &[&str],
    fragments: &[&str],
) -> Result<(), anyhow::Error> {
    for row in value
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("invalid catalog rows"))?
    {
        type_row(row, identifiers, fragments)?;
    }
    Ok(())
}

#[cfg(test)]
mod test_type_fields {
    use crate::extract::catalog::type_fields;

    #[test]
    fn types_each_row_and_accepts_empty_catalogs() {
        let mut rows = serde_json::json!([{"name":"a"}, {"name":"b"}]);
        type_fields(&mut rows, &["name"], &[]).unwrap();
        assert_eq!(
            rows,
            serde_json::json!([{"name":{"identifier":"a"}}, {"name":{"identifier":"b"}}])
        );
        let mut empty = serde_json::json!([]);
        type_fields(&mut empty, &["name"], &[]).unwrap();
        assert_eq!(empty, serde_json::json!([]));
    }

    #[rstest::rstest]
    #[case::not_array(serde_json::json!({}), "invalid catalog rows")]
    #[case::invalid_row(serde_json::json!([{}]), "missing catalog field name")]
    fn rejects_invalid_catalogs(#[case] mut rows: serde_json::Value, #[case] expected: &str) {
        assert_eq!(
            type_fields(&mut rows, &["name"], &[])
                .unwrap_err()
                .to_string(),
            expected
        );
    }
}

fn type_row(
    row: &mut Value,
    identifiers: &[&str],
    fragments: &[&str],
) -> Result<(), anyhow::Error> {
    for key in identifiers {
        row[*key] = identifier(required_string(row, key)?);
    }
    for key in fragments {
        if !row[*key].is_null() {
            row[*key] = fragment(required_string(row, key)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_type_row {
    use crate::extract::catalog::type_row;

    #[test]
    fn distinguishes_identifiers_fragments_and_optional_fragments() {
        let mut row =
            serde_json::json!({"name":r#"a"b"#, "sql":"x < 2", "nullable":null, "untouched":true});
        type_row(&mut row, &["name"], &["sql", "nullable", "absent"]).unwrap();
        assert_eq!(
            row,
            serde_json::json!({
                "name":{"identifier":r#"a"b"#}, "sql":{"fragment":"x < 2"},
                "nullable":null, "untouched":true
            })
        );
    }

    #[rstest::rstest]
    #[case::identifier(serde_json::json!({"name":null}), vec!["name"], vec![])]
    #[case::fragment(serde_json::json!({"sql":false}), vec![], vec!["sql"])]
    fn rejects_non_string_fields(
        #[case] mut row: serde_json::Value,
        #[case] identifiers: Vec<&str>,
        #[case] fragments: Vec<&str>,
    ) {
        assert!(type_row(&mut row, &identifiers, &fragments).is_err());
    }
}

pub(super) fn write_json(path: &Path, value: &Value) -> Result<(), anyhow::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod test_write_json {
    use crate::extract::catalog::write_json;

    #[test]
    fn writes_pretty_json_with_a_final_newline_and_replaces_existing_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.json");
        write_json(&path, &serde_json::json!({"longer":"previous contents"})).unwrap();
        write_json(&path, &serde_json::json!({"a":[1,true]})).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            concat!(
                r#"{
  "a": [
    1,
    true
  ]
}"#,
                "\n"
            )
        );
    }

    #[test]
    fn propagates_filesystem_errors() {
        let directory = tempfile::tempdir().unwrap();
        let error = write_json(directory.path(), &serde_json::json!({})).unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
    }
}
