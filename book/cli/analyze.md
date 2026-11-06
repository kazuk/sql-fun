# Analyze SQL

`sql-fun analyze` statically analyzes SQL without extracting queries from Rust
or creating query lockfiles. It uses `sql-fun-server` and an installed builtin
catalog; it does not execute the SQL or require a live database or `cargo-expand`.

```text
sql-fun [COMMON OPTIONS] analyze [OPTIONS] [SQL]
```

Use [common options](README.md#common-options) before `analyze`. Exactly one of
`SQL` or `--sql-file <FILE[#JSONPath]>` is required. `--schema-level` accepts
`builtin`, `default`, `extensions`, or `project` (the default).
`--no-column-access` omits access analysis, and `--diagnostics-to-stderr` copies
SQL diagnostics to stderr. `-h` / `--help` prints help. These command-specific
options have no environment variable equivalents.

The datasource is selected by the global `--datasource` option, then by
`SQL_FUN_DATASOURCE`, or otherwise by the name `default`. Configuring a
datasource named `default` selects it for use when no other name is supplied.
The selected datasource must exist and have valid metadata. The command reads
its project metadata; use `--metadata-file` to select that file, or the common
metadata configuration and discovery apply. Place global options before
`analyze`.

```sh
sql-fun --metadata-file Cargo.toml --datasource app analyze 'SELECT 1 AS result'
sql-fun --metadata-file Cargo.toml --datasource app analyze --sql-file queries.sql
sql-fun --metadata-file Cargo.toml --datasource app analyze --sql-file 'queries.toml#$.queries'
```

Supply exactly one input: the positional argument is literal SQL, while
`--sql-file` explicitly selects a file. Quote SQL for your shell, and use `--`
before positional SQL beginning with `-`. A positional filename is treated as
SQL text; change existing file invocations to use `--sql-file`.

For `--sql-file`, without `#`, the file is UTF-8 SQL text, regardless of
filename extension.
With `#`, the file must have a `.json`, `.toml`, or `.yaml` extension, and the
suffix is a JSONPath selecting exactly one node. A selected string is one input;
a selected string array is an ordered sequence of independent inputs. Zero or
multiple selected nodes, empty arrays, mixed arrays, and other selected types
are input errors. Multiple selected nodes are never flattened. Relative filenames
are resolved from the command's working directory.

Each selected SQL string can contain multiple statements. Each input starts
with one source identity and an overall status: `OK` means no warnings or
errors, `WARN` means warnings without errors, and `ERROR` means at least one
error. Advice alone is `OK`. With multiple statements the highest severity is
used, and zero-based statement headings identify each statement. An operational
error is identified separately from SQL diagnostics. Status colors are green,
yellow, and red respectively. `--term-color auto` enables color when the output
stream is a terminal; `never` disables it and `ansi` forces it.

The report shows executable SQL, diagnostics, and nonempty tables of parameters,
result columns, and relation/column accesses. Parameters include their SQL
placeholder index, name, type, and nullability; result columns appear in ordinal
order with their name, type, and nullability. Array types have a `[]` suffix.
Unavailable SQL and unresolved types are marked explicitly. Tables use the width
of their contents, so a wide table may wrap at the terminal edge.

SQL diagnostics and their source highlighting are included in the stdout report.
Use `--diagnostics-to-stderr` to additionally copy SQL diagnostics to stderr;
status headers, SQL, and tables are not copied. See the
[diagnostic reference](../diagnostics/README.md) for code meanings and suggested
corrections. Error diagnostics do not stop later inputs. Each input uses the same
selected schema base, without inheriting changes from preceding inputs.

## Relation and column access

By default, `analyze` also requests access analysis and displays the returned
facts in a `Column access` section for each statement. Each record shows a
resolved relation, its kind, an optional column, and the purpose of the access:

```text
  Column access
    Relation/column    Kind   Role
    public.events      Table  ReadRows
    public.events.id   Table  SelectFilter
    public.events.id   Table  SelectOutput
```

A record without a column describes a relation-level action. Names preserve
returned catalog/schema qualification. Records are sorted by relation identity,
column, relation kind, and purpose; every returned role is shown even when the
same column has several roles. Purpose names describe why the top-level statement
needs the access, including references inside nested expressions and subqueries.

See the [SQL access-purpose reference](../access-kinds.md) for the complete
vocabulary, SQL examples, and analysis boundaries.

To avoid the additional server-side analysis and omit its display, use:

```sh
sql-fun --metadata-file Cargo.toml --datasource app analyze \
  --sql-file queries.sql --no-column-access
```

This option preserves ordinary diagnostics, parameters, and result columns.
When no accesses are returned, the section is omitted. Access facts can also be
returned for incomplete statements; they do not guarantee that every access was
resolved.

## Parameter bindings

Analysis finalizes the application's input binding contract. If SQL inference
and supported explicit input type hints leave several possible input types or
no type restriction, `parameter_type_unresolved` is an error. The statement is
incomplete and `analyze` exits with status 1, while later inputs still run.
Conflicting requirements instead retain their conflict diagnostics.

This error does not mean PostgreSQL rejects the SQL. For example,
`SELECT $1::text AS value` can be prepared by PostgreSQL but leaves the
application's input binding unresolved here. Its `value` column is still
reported as `pg_catalog.text`; known output columns do not establish a complete
input binding. Analysis does not automatically choose PostgreSQL PREPARE's
inferred parameter signature.

Use the supported named-placeholder input hint to select the source type:

```sql
SELECT ${src:text}::text AS value;
```

This completes with a `text` input binding. `${src}::text` supplies a result cast
and may leave multiple input types admissible. Similarly, for a function
`record_fun(src text) RETURNS record`, the following explicit input hint
resolves the binding while preserving the declared result columns:

```sql
SELECT a,b FROM record_fun(${src:text}::text) AS r(a int,b text);
```

Hints must be compatible with every use of the parameter. Use either named
placeholders or native numbered parameters in one input, never both. See
[parameter analysis](../sql-analysis/expressions/parameters.md) for the distinction
between admissible type domains and the public binding contract.

## Schema loading

Both `analyze` and `test` accept `--schema-level`:

| Value | Loaded schema |
| --- | --- |
| `builtin` | Builtin catalog/type/function/operator definitions only |
| `default` | Builtins plus the server's bundled `schema.sql` |
| `extensions` | Default SQL plus configured extension SQL, in configured order |
| `project` | Extensions plus the selected project schema dump |

The default is `project`. Lower levels do not require or access SQL files from
higher levels. In particular, the project dump need not exist for `builtin`,
`default`, or `extensions`; `builtin` also does not read bundled `schema.sql`.
Datasource configuration and the installed builtin catalog are still required.
The existing initialization API currently supports PostgreSQL schema contexts.

Server connection uses the same `USER`, `SQL_FUN_SERVER`, and
`SQL_FUN_SERVER_COMMAND` settings as [`sql-fun api`](../api/cli-api.md).

The command exits with status 0 if all analysis completes without error-level
diagnostics. Warnings and advice alone succeed. Error diagnostics and execution
errors produce status 1; invalid CLI usage produces status 2.
