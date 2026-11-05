# Installation

`sql-fun` uses `tokio-postgres` for PostgreSQL and `rusqlite` for SQLite.
SQLite support is under development and requires the `sqlite` feature.

`sql-fun` is a procedural macro crate. You can install it using `cargo add`:

```console
cargo add sql-fun
cargo install sql-fun-cli sql-fun-server
cargo install --locked cargo-expand
```

The `cargo install` commands above build the CLI and server from source.

## Install prebuilt CLI and server executables

Starting with the first project-wide v0.1 release, the CLI and server release
crates support [cargo-binstall](https://github.com/cargo-bins/cargo-binstall).
Install cargo-binstall first using its installation instructions. Choose the
**component crate versions** listed in the product release notes; the project
tag is independent of these versions. `sql-fun-cli` installs `sql-fun`, and
`sql-fun-server` installs `sql-fun-server` from the same release archive.

For example, if a stable release lists CLI `0.2.0` and server `0.1.1`, install
those exact versions with:

```console
cargo binstall --strategies crate-meta-data --targets x86_64-unknown-linux-gnu \
  --version '=0.2.0' sql-fun-cli
cargo binstall --strategies crate-meta-data --targets x86_64-unknown-linux-gnu \
  --version '=0.1.1' sql-fun-server
```

For a snapshot listing CLI `0.2.0-snapshotN` and server `0.1.1-snapshotN`, use
`--version '=0.2.0-snapshotN'` and `--version '=0.1.1-snapshotN'` respectively,
replacing `N` with the selected release's actual suffix. These are version
examples, not a promise that those versions have been published. Historical
crate versions published before this support cannot acquire the metadata.

These commands retrieve project-provided binaries only and fail if unavailable;
they do not fall back to compilation or third-party binaries. Prebuilt support
is limited to `x86_64-unknown-linux-gnu`, built and validated on Ubuntu 24.04.
Compatibility with older distributions is not promised. Archives contain the
default-feature executables; use source installation for custom features or
other targets. Ensure Cargo's binary directory is on `PATH`.

The procedural macro remains a Cargo dependency. Command-specific prerequisites
such as `cargo-expand` for `sql-fun check` and PostgreSQL client tools for
extraction still apply, as described below.

## Install from source

Building the analyzer from source requires native compilation tools: a C compiler
and assembler for stack growth, plus Clang for PostgreSQL parser bindings. On
Ubuntu, install `build-essential` and `clang` before the Cargo commands above.
Cross-compilation requires tools for the target platform. These are build
prerequisites; the installed CLI does not invoke them for stack growth.

For SQLite, enable the feature on the proc-macro, CLI, and server:

```console
cargo add sql-fun --features "sqlite"
cargo install sql-fun-cli sql-fun-server --features "sqlite"
cargo install --locked cargo-expand
```

For the planned application-runtime extraction workflow, see
[SQLite project bootstrap](sqlite-bootstrap.md). That page distinguishes current
setup from the intended helper and project-local engine builtin snapshot.

`sql-fun-cli` for CLI tool for `sql-fun`. `sql-fun-server` provides cache for
schema objects.

`sql-fun check` requires `cargo-expand` for both PostgreSQL and SQLite.
It is not installed automatically with `sql-fun-cli`. Ensure Cargo's binary
directory is on `PATH`
and verify the installation before running `sql-fun check`:

```console
cargo expand --version
```

`sql-fun check` reports a missing prerequisite and exits with an error when
`cargo-expand` is not executable on `PATH`. Install it with the command above
and add Cargo's binary directory to `PATH` before retrying. It is a prerequisite
of this command, not a Debian package installation dependency.

`sql-fun api` requires the installed `sql-fun-server` executable on `PATH` or an
explicit `SQL_FUN_SERVER` executable path, and a user selected by `--user` or
`USER`. A `--server-command`/`SQL_FUN_SERVER_COMMAND` launch override requires
`/bin/sh`. Cargo is required only when the selected launch command invokes it.
See [the API command reference](cli/api.md) for streaming examples and connection
settings.

Specify this in your Cargo.toml as follows:

```toml
[package.metadata.database.default]
engine = "PostgreSQL"
dialect = "postgres"
version = "17"
connector = "tokio-postgres"
```

For PostgreSQL queries using `database.schema.table.column`, configure
`database-name` explicitly as described in
[qualified column references](sql-analysis/expressions/names.md).
The datasource selection name does not supply the database identity.

- `default`: default database for this application
- `engine`: identifies the database engine. Use "PostgreSQL" for PostgreSQL or
  "sqlite" for SQLite.
- `dialect`: explicitly selects the SQL dialect used for parsing and semantic
  interpretation. This required choice is not inferred from `engine`; use
  "postgres" for PostgreSQL or "SQLite" for SQLite.
- `version`: PostgreSQL target major, matching the installed builtin metadata.
- `connector`: identifies the application database connector library.
  - When `engine` is "PostgreSQL": Can be "tokio-postgres".
  - When `engine` is "sqlite": Can be "rusqlite".

To expand table-dependent wildcard projections in PostgreSQL executable SQL,
explicitly enable the option for the selected datasource:

```toml
[package.metadata.database.default.rewrite_options]
expand_asterisk = true
```

Omitting this option, using an empty `rewrite_options` table, or setting
`expand_asterisk = false` preserves existing SQL output. See
[wildcard projection expansion](sql-analysis/statements/select.md#wildcard-projection-expansion)
for its scope and schema requirements. SQLite expansion is not available.

You can use multiple database in application like a bellow.

```toml
[package.metadata.database]
main = {
    engine = "PostgreSQL",
    dialect = "postgres",
    version = "17",
    connector = "tokio-postgres"
}
edge = {
    engine = "sqlite",
    dialect = "SQLite",
    connector = "rusqlite"
}
```

`main` and `edge` using in query.

```rust
#[sql_fun::sql_query_one(
    datasource="main", // query to postgres
    sql=#"
      select u.name
        from users u
        where u.id={id}
    "#
)]
pub async fn select_user_name_by_user_id(
    client: pg::Client,
    id: u64
) -> Result<String, QueryError> {}

#[sql_fun::sql_statement(
    datasource= "edge", // execute statement to sqlite
    sql=#"insert into profile (name) values(${name})"#
)]
pub fn insert_profile(
    client: rusqlite::Connection,
    name: &str
) -> Result<usize, rusqlite::Error> {}
```

When `datasource` attribute not specified in code, default value is "default".

If you're starting a new project, you may also need to install the required
dependencies:

```console
cargo add tokio --futures full
cargo add tokio-postgres futures-util
```

## Configure known PostgreSQL roles

For offline ownership analysis, list known role names in the selected datasource
of your project's `Cargo.toml` (or the metadata file you explicitly select):

```toml
[package.metadata.database.app]
engine = "PostgreSQL"
dialect = "postgres"
version = "18"
roles = ["app_owner", "Reporting Owner"]
```

Each string is an exact catalog name, without SQL identifier quoting. For example,
`"app_owner"` matches SQL `app_owner`, `APP_OWNER`, and `"app_owner"`;
`"Reporting Owner"` matches SQL `"Reporting Owner"`. TOML quoting delimits strings;
it does not add SQL quotes. Names retain case, spaces, Unicode, dots, and literal
quote characters. A role name containing a double quote, such as `a"b`, can be
written as the TOML literal string `'a"b'` and referenced in SQL as `"a""b"`.

Role names are global, independent of schemas and search paths. Configure names
needed by your selected datasource; sql-fun does not discover roles from a server,
schema names, or your operating-system identity. Empty names, NUL characters,
duplicate exact names, and values other than arrays of strings are rejected.

Omitting `roles` leaves the inventory unspecified and preserves existing analysis
behavior. `roles = []` explicitly supplies no roles. Both report unavailable named
owners when an ALTER OWNER statement needs a role that was not supplied.
Configuration supplies role identities for analysis; it does not create roles,
connect to PostgreSQL, or validate membership and permissions. See
[ownership analysis](sql-analysis/statements/alter-owner.md).

## Initialize PostgreSQL builtin metadata

PostgreSQL analysis requires builtin metadata matching your target PostgreSQL
version, even for ordinary schema definitions using types such as `int`.
This metadata is required independently of any optional application schema dump.

Install PostgreSQL client tools (`psql`). For the Docker source, also
install Docker CLI and provide access to a Docker daemon, then run:

```console
sql-fun init
```

Initialization also requires a valid diagnostic theme. Downloading the default
requires `curl` or `wget` with TLS support and trusted certificates. See
[initialization](cli/init.md#diagnostic-theme) for retaining or editing a theme.

Choose the PostgreSQL engine or a configured PostgreSQL datasource, the home
directory, a source, and the versions you need (currently 17 and 18). For a
non-interactive initialization of a specific version:

```console
sql-fun init --yes --install-default-theme --engines postgres \
  --sql-fun-home "$HOME/.sql-fun" \
  --docker --engine-versions 17
```

Set your datasource's `version` to the matching major. Initialization stores
metadata under `<chosen-home>/postgres/<version>/` (suggested home `~/.sql-fun`),
which normal analysis loads. It preserves valid installations on rerun.

You can also initialize from an existing PostgreSQL server using
`--database-url`; this checks the server's version against your explicit choice
and requires no Docker access. See [initialization](cli/init.md) for connection
settings, selecting multiple versions, custom homes, and retrying failures.
Detailed catalog and extension extraction workflows are described in
[extraction](cli/extract.md).

## PostgreSQL extraction prerequisites

[`sql-fun extract`](cli/extract.md) includes its catalog queries, SQL templates,
and extension export script in the installed CLI. Both connection and Docker
extraction require the PostgreSQL client tools (`psql`) on `PATH`. Connection
extraction needs network access and appropriate libpq authentication/TLS settings
(for example, `sslrootcert` and `sslmode=verify-full` for CA and hostname checking).
Docker extraction additionally requires Docker CLI and daemon access.
The server administrator's extension export requires that installation's `pg_config`,
POSIX shell utilities, and GNU tar. No `hbs`, SSH integration, or sql-fun
installation is required on the database host.

## Setting environment variables

- `SQL_FUN_SCHEMA` : optional and default is `develop`
  - Used to determine which schema dump file to load and which `sql-fun` output
    file to generate.

Additionally, you may want to set environment variables used by `libpq` for
smooth development.

- See:
  [PostgreSQL: Documentation: 17: 32.15. Environment Variables](https://www.postgresql.org/docs/current/libpq-envars.html)

## Providing a schema dump (optional, for schema-aware validation)

If you're working with a new database and have no existing schema,
you can manually write a DDL SQL file and place it as `schema.develop.sql`.
(develop comes from the SQL_FUN_SCHEMA environment variable.)

If you're working with an existing database, you can generate the schema dump
like this:

```console
pg_dump -s > schema.develop.sql
```

If no schema file is provided, sql-fun will still work — but schema-aware
validation will be disabled, and you'll see a warning during compilation:

```text
sql-fun: schema not loaded
```

To disable this warning, you can set `schema_checking = false` in your
`Cargo.toml`:

```toml
[package.metadata.database]
default = {
    schema_checking = false
    engine="PostgreSQL"
    dialect="postgres"
    version="17"
    connector = "tokio-postgres"
}
```

## Managing Your Schema

`sql-fun` does not manage your database schema or migrations.

You are responsible for keeping your schema in sync with your application.  
Use your preferred tools or workflows — whether it's manual SQL, `sqldef`,
`sqlx-cli`, `golang-migrate`, or anything else.

If you want schema-aware validation, provide a schema dump using `pg_dump -s`.  
This lets `sql-fun` validate your queries at compile time, but it never assumes
ownership of the schema itself.

> ⚠️ Make sure schema definition is part of your project or CI pipeline.  
> `sql-fun` focuses on code generation, not schema evolution.

`sql-fun analyze` and `sql-fun test` use the same installed server and builtin
catalogs as the API. They use the selected datasource (or the configured
`default` datasource), but do not require `cargo-expand` or a live database.
See [analyze](cli/analyze.md) and [test](cli/test.md) for input selection,
schema levels, and query contracts.
