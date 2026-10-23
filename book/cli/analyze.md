# Analyze SQL files

`sql-fun analyze` statically analyzes SQL without extracting queries from Rust
or creating query lockfiles. It uses `sql-fun-server` and an installed builtin
catalog; it does not execute the SQL or require a live database or `cargo-expand`.

Select the datasource explicitly using `--datasource` or `SQL_FUN_DATASOURCE`.
This selection authorizes reading that datasource's project metadata. You can
select the metadata file with `--metadata-file`; otherwise the common metadata
configuration applies. The command does not choose an available datasource for you.

```sh
sql-fun --metadata-file Cargo.toml --datasource app analyze queries.sql
sql-fun --metadata-file Cargo.toml --datasource app analyze 'queries.toml#$.queries'
```

Without `#`, the input is UTF-8 SQL text, regardless of filename extension.
With `#`, the file must have a `.json`, `.toml`, or `.yaml` extension, and the
suffix is a JSONPath selecting exactly one node. A selected string is one input;
a selected string array is an ordered sequence of independent inputs. Zero or
multiple selected nodes, empty arrays, mixed arrays, and other selected types
are input errors. Multiple selected nodes are never flattened. Relative filenames
are resolved from the command's working directory.

Each selected SQL string can contain multiple statements. Diagnostics are shown
for every statement with SQL source context. Error diagnostics do not stop later
inputs. Each input uses the same selected schema base, without inheriting changes
from preceding inputs.

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
`SQL_FUN_SERVER_COMMAND` settings as [`sql-fun api`](api.md).

The command exits with status 0 if all analysis completes without error-level
diagnostics. Warnings and advice alone succeed. Error diagnostics and execution
errors produce status 1; invalid CLI usage produces status 2.
