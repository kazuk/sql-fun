# Configure a non-bundled PostgreSQL extension

Use this guide to make pgvector v0.8.7 types, functions, and operators
available to offline SQL analysis. Follow the
[installation guide](../installation.md) to set up the sql-fun CLI, server,
and PostgreSQL 18 builtin catalog. Use a build that includes the
[operator lookup fix](https://github.com/kazuk/sql-fun/pull/1297).

## Stage the extension files

Start in a scratch directory. Set `sql_fun_home` to the absolute path of
your sql-fun home containing the PostgreSQL 18 builtin catalog. If an
older catalog was extracted from a database with pgvector installed,
regenerate it with an updated sql-fun in a fresh home before staging the
extension files.

Obtain the files from the
[pgvector v0.8.7 source](https://github.com/pgvector/pgvector/tree/v0.8.7)
and place them side by side in the PostgreSQL 18 extension directory.
The versioned installation script is a copy of upstream `sql/vector.sql`.

```sh
git clone --depth 1 --branch v0.8.7 https://github.com/pgvector/pgvector.git
extension_dir="$sql_fun_home/postgres/18/extensions"
mkdir -p "$extension_dir"
cp pgvector/vector.control pgvector/sql/vector--*--*.sql "$extension_dir/"
cp pgvector/sql/vector.sql "$extension_dir/vector--0.8.7.sql"
```

For another catalog major, use the corresponding directory under
`<sql-fun-home>/postgres/<major>/extensions/`. To select a different
directory explicitly, use `--postgres-extensions-dir` or
`SQL_FUN_EXTENSIONS_DIR`. See the [CLI common options](../cli/index.md).

## Select pgvector in a datasource

Save this example as `Cargo.toml` in the scratch directory.

```toml
[package]
name = "pgvector-offline-example"
version = "0.1.0"

[package.metadata.database.vector_example]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
connector = "tokio-postgres"
extensions = [{ name = "vector" }]
```

This selection uses `default_version` from `vector.control`, which is
`0.8.7` for these files. To pin that version, use
`extensions = [{ name = "vector", version = "0.8.7" }]`. sql-fun loads the
installation script and any updates needed to reach the selected version.

## Analyze a distance expression

Save the query and run [`analyze` at the extensions schema level](../cli/analyze.md#schema-loading).

```sh
printf '%s\n' "SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance;" > distance.sql
sql-fun --sql-fun-home "$sql_fun_home" --metadata-file "$PWD/Cargo.toml" \
  --datasource vector_example analyze --schema-level extensions --sql-file distance.sql
```

The command succeeds and reports a result column named `distance` with
type `pg_catalog.float8`. The
[validation record](https://github.com/kazuk/sql-fun/blob/442c4ccc/docs/design-docs/pgvector-offline-validation.md)
also shows the cast and function probes for this setup.

sql-fun interprets the extension's SQL declarations offline to resolve
types, functions, and operators. Installing the extension in a database
and executing its functions are PostgreSQL operations. See the
[CREATE EXTENSION reference](../sql-analysis/statements/create-extension.md)
for the scope of extension analysis.
