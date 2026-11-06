# sql-fun Command Line Interface

The `sql-fun` executable initializes development support, analyzes SQL and Rust
project queries, checks SQL contracts, extracts PostgreSQL metadata, and connects
to the server API. PostgreSQL and feature-gated SQLite are supported; consult
the [SQL Analysis Reference](https://kazuk.github.io/sql-fun/sql-analysis.html)
for coverage and limitations.

## Installation

```console
cargo install sql-fun-cli
cargo install --locked cargo-expand
cargo expand --version
```

`sql-fun check` requires `cargo-expand`; installing the CLI does not install it.
Ensure Cargo's binary directory is on `PATH`. See the
[installation guide](https://kazuk.github.io/sql-fun/installation.html) for
server installation, SQLite features, native build prerequisites, and project
configuration.

## Commands

Run `sql-fun --help` or `sql-fun <command> --help` for the installed version.
The [CLI reference](https://kazuk.github.io/sql-fun/cli/index.html) describes
common options and external commands. Each built-in command has a reference:

- [`init`](https://kazuk.github.io/sql-fun/cli/init.html): prepare development support.
- [`analyze`](https://kazuk.github.io/sql-fun/cli/analyze.html): analyze SQL without executing it.
- [`test`](https://kazuk.github.io/sql-fun/cli/test.html): check SQL diagnostics and result-column contracts.
- [`check`](https://kazuk.github.io/sql-fun/cli/check.html): analyze SQL extracted from a Rust project.
- [`extract`](https://kazuk.github.io/sql-fun/cli/extract.html): manage PostgreSQL metadata:
  - [`extract catalog`](https://kazuk.github.io/sql-fun/cli/extract/catalog.html).
  - [`extract export-script`](https://kazuk.github.io/sql-fun/cli/extract/export-script.html).
  - [`extract import-extensions`](https://kazuk.github.io/sql-fun/cli/extract/import-extensions.html).
- [`api`](https://kazuk.github.io/sql-fun/cli/api.html): exchange JSON Lines requests with the server.
