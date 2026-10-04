use crate::extract::render::{Renderer, fragment, identifier};

use std::{collections::BTreeMap, path::Path, time::Duration};

use ::serde_json::Value;
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

pub(crate) async fn detect(
    configuration: &str,
) -> Result<sql_fun_core::PostgresVersion, anyhow::Error> {
    let output = run_psql(
        configuration,
        "SHOW server_version_num; SHOW server_version;\n",
    )
    .await?;
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

pub(super) async fn extract(
    configuration: &str,
    output: &Path,
    provenance: &Value,
) -> Result<(), anyhow::Error> {
    let identity = detect(configuration).await?;
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
    ];
    let mut sql = String::from(
        "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\nSET LOCAL search_path = pg_catalog; SET LOCAL standard_conforming_strings = on; SET LOCAL extra_float_digits = 3; SET LOCAL timezone = 'UTC'; SET LOCAL statement_timeout = '60s';\nSHOW server_version_num;\n",
    );
    for query in queries {
        sql.push_str(&format!(
            "SELECT ({})::text;\n",
            query.trim().trim_end_matches(';')
        ));
    }
    sql.push_str("COMMIT;\n");
    let results = run_psql(configuration, &sql).await?;
    let mut lines = results.split_terminator('\0');
    let version: u32 = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing server version"))?
        .parse()?;
    anyhow::ensure!(
        version / 10_000 == identity.major(),
        "server version changed during extraction"
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
    let renderer = Renderer::new()?;
    let mut schema =
        String::from("-- PostgreSQL builtin schema; generated by sql-fun extract, format 1.\n");
    let mut domains = next_catalog(&mut lines)?;
    type_fields(&mut domains, &["schema", "name"], &["definition"])?;
    schema.push_str(&renderer.render("domains", &domains)?);
    let columns = next_catalog(&mut lines)?;
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
    let mut functions = next_catalog(&mut lines)?;
    type_fields(
        &mut functions,
        &["schema", "name"],
        &["arguments", "result"],
    )?;
    schema.push_str(&renderer.render("functions", &functions)?);
    let mut views = next_catalog(&mut lines)?;
    type_fields(&mut views, &["schema", "view"], &["definition"])?;
    schema.push_str(&renderer.render("views", &views)?);
    std::fs::write(output.join("schema.sql"), schema)?;
    anyhow::ensure!(lines.next().is_none(), "unexpected psql catalog result");
    let mut metadata = serde_json::json!({
        "format_version": 1, "server_version_num": version,
        "extractor_version": env!("CARGO_PKG_VERSION"), "provenance": provenance,
        "scope": "bootstrap-pg_catalog-and-information_schema"
    });
    if !identity.is_stable() {
        metadata["version_identity"] = identity.to_string().into();
    }
    write_json(&output.join("metadata.json"), &metadata)?;
    Ok(())
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, anyhow::Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing catalog field {field}"))
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

pub(super) fn write_json(path: &Path, value: &Value) -> Result<(), anyhow::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}
