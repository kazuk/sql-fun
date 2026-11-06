# CLI Reference

The `sql-fun` executable provides commands for initialization, SQL analysis,
contract testing, project checks, catalog extraction, and the server API bridge.
Run `sql-fun --help`, `sql-fun <command> --help`, or `sql-fun help <command>`
for the installed version. For nested help, use for example
`sql-fun help extract catalog`. Help exits successfully.
With no command, the CLI reports that no subcommand was supplied and exits 0.

- [`init`](init.md): prepare development support.
- [`analyze`](analyze.md): analyze SQL without executing it.
- [`test`](test.md): check SQL diagnostics and result-column contracts.
- [`check`](check.md): analyze queries extracted from a Rust project.
- [`extract`](extract.md): extract PostgreSQL catalogs or import extension files.
- [`api`](api.md): exchange JSON Lines requests with the server.

## Common options

Use common options **before** the subcommand, for example:

```sh
sql-fun --metadata-file Cargo.toml --datasource app analyze 'SELECT 1'
```

For an option with an environment variable, a command-line value takes precedence
over the variable; a listed default applies when neither is supplied. These
options are parsed at the top level, but their effects depend on the command.
In particular, `api` and `extract` do not use the common project and schema
settings. `init` has its own `--sql-fun-home` and `--datasource` options after
`init`, with different consent rules; see [init](init.md).

| Option | Environment | Default and purpose |
| --- | --- | --- |
| `--sql-fun-home <PATH>` | `SQL_FUN_HOME` | `~/.sql-fun`; home for installed catalogs and diagnostic themes. An explicitly configured home must exist when used for analysis. |
| `--metadata-format <FORMAT>` | `SQL_FUN_METADATA_FORMAT` | `TOML`; the default metadata filename is defined for TOML. |
| `--metadata-file <PATH>` | `SQL_FUN_METADATA_FILE` | `Cargo.toml`; metadata for the selected project. |
| `--schema <NAME>` | `SQL_FUN_SCHEMA` | `develop`; select `sqlfun.<NAME>.sql` beside the metadata file. |
| `--schema-file <PATH>` | `SQL_FUN_SCHEMA_FILE` | No override; an explicit schema file takes precedence over `--schema`. |
| `--sql-dialect <DIALECT>` | `SQL_FUN_DIALECT` | No override; use the datasource dialect. The supported explicit override is `postgresql` (case-insensitive). |
| `--cte-catalog-dir <PATH>` | `SQL_FUN_CTE_CATALOG_DIR` | No override; select a reusable CTE catalog directory. |
| `--pg_search_path <PATH>` | `SQL_FUN_PG_SEARCH_PATH` | Datasource search path, or `public` if unset. |
| `--postgres-version <VERSION>` | `SQL_FUN_POSTGRES_VERSION` | Datasource version; selects PostgreSQL builtin metadata. |
| `--postgres-extensions <LIST>` | `SQL_FUN_EXTENSIONS` | Datasource extensions; comma-separated `name` or `name@version` entries. |
| `--builtin-schema-dir <PATH>` | `SQL_FUN_BUILTIN_SCHEMA` | No override; select a builtin catalog directory instead of the version-based directory. |
| `--postgres-extensions-dir <PATH>` | `SQL_FUN_EXTENSIONS_DIR` | No override; select the PostgreSQL extension SQL directory. |
| `--log-level <LEVEL>` | `SQL_FUN_LOG_LEVEL` | `info`; help describes `trace`, `debug`, `info`, `warn`, and `error`. |
| `--log-file <PATH>` | `SQL_FUN_LOG_FILE` | No file; log-file setting. |
| `--output-format <FORMAT>` | `SQL_FUN_OUTPUT_FORMAT` | `text`; help describes `text` and `json`. |
| `--term-color <MODE>` | `SQL_FUN_TERM_COLOR` | `auto`; `auto`, `never`, or `ansi`. |
| `--highlighter-theme <PATH>` | `SQL_FUN_HIGHLIGHTER_THEME` | Default theme under the selected home when color is enabled; relative paths are under its `highlighter` directory. |
| `--verbose` | — | Off; accepted verbose flag. |
| `--no-builtin-tables` | `SQL_FUN_NO_BUILTIN` | Off; accepted builtin-table flag. |
| `--datasource <NAME>` | `SQL_FUN_DATASOURCE` | `default`; select a datasource for commands that use it. |
| `--mode <MODE>` | `SQL_FUN_MODE` | No override; proc-macro mode setting. |
| `-h`, `--help` | — | Print help. |
| `-V`, `--version` | — | Print the CLI version. |

The CLI accepts `--output-format` and `--log-level` as strings and forwards
settings where applicable; it does not validate them against the values in help.
The built-in commands do not implement a JSON output mode through
`--output-format`, and `--verbose` and `--no-builtin-tables` have no effect on
the built-in command implementations. `api` uses `RUST_LOG` for its own tracing;
see its [reference](api.md).

A relative metadata path is searched from the current directory upward, stopping
when the owner changes; an absolute path is checked directly. Home paths accept
absolute paths, paths relative to the working directory, and `~/` paths. These
settings do not themselves authorize every command to inspect project files:
[`init`](init.md) requests project access only through its documented selections.

## External commands

An unrecognized subcommand is looked up on `PATH` as `sqlfun-<name>` or
`sqlfun-<name>-daemon`. If neither or both are found, execution fails. The
captured subcommand name and remaining arguments are passed to the selected
executable. The CLI also loads the selected project metadata to prepare its
environment before launching it. Its result becomes the CLI result, subject to
the CLI's process and exit-status handling. The built-in commands above take
precedence over external names.
