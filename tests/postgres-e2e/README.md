# PostgreSQL execution E2E

For contributors testing the complete public SQL workflow, run from the
repository on a Linux host with Docker Engine access:

```sh
tests/postgres-e2e/run.sh --local
```

The command builds the local CLI/server, starts disposable PostgreSQL 18,
applies `schema.sql` with `psql`, generates a fresh lockfile with `sql-fun check`,
validates the generated CRUD/query inventory against `inventory.toml`, checks
validation without writes and deterministic regeneration, and executes the
ordinary Cargo consumer. Parameters explicitly declare their
SQL types using `${id:bigint}` and `${name:text}`. Rust assertions verify bound
integer and text values through `INSERT`, `SELECT`, `UPDATE`, and `DELETE`,
including RETURNING, a self-join, a CTE, a scalar subquery and a view.
Failure in any phase makes the command fail. Containers, server processes, generated
lockfiles, inventories, and temporary state are cleaned up on exit or handled interruption.
Dependency artifacts persist in the repository's `target/` (or an explicitly
supplied `CARGO_TARGET_DIR`) and are reused across queries and subsequent local
runs. The consumer itself is rebuilt against each run's generated lockfile.
CI discards build artifacts after E2E execution.

In addition to the [contributor setup](../../CONTRIBUTING.md#getting-started),
install the PostgreSQL client (`sudo apt-get install postgresql-client` on
Ubuntu), and ensure `docker`, `psql`, `cargo`, and `cargo-expand` are on `PATH`.
Docker must publish container ports on the local host's loopback interface.
The development builder has no Docker socket; run this command on the host,
with the Rust/native build tools described in the contributor setup.

CI uses the same runner with the distribution artifact already downloaded by
`install-validation`:

```sh
tests/postgres-e2e/run.sh --installed /usr/bin/sql-fun /usr/bin/sql-fun-server
```

This mode requires the installed binaries and consumer build prerequisites;
it does not build replacement CLI/server binaries. Each run owns an isolated
database/container and server namespace, so existing database contents are
irrelevant. Concurrent runs use distinct lockfiles and share Cargo's build
directory locking. No seeded rows or result assertions in shell are needed.

The runner starts with an absent test-owned `SQL_FUN_HOME` and runs
`sql-fun init --yes --install-default-theme` against its disposable PostgreSQL 18
server. This installs builtin metadata and downloads the theme from the published
GitHub Pages manual. These online checks require access to that site and `curl`
or `wget` with TLS support; download failure fails the E2E without a fixture
fallback. The existing CLI download limits apply.

Before checking the executable queries, the runner checks the isolated
`theme-diagnostic` example with `--term-color ansi` and verifies its unresolved
parameter diagnostic, failure status, and ANSI rendering. The successful
`check → cargo run` path then uses the same initialized home. The invalid example
is extracted for analysis only and is never executed as a database query.

The inventory's relation/column access maps group responsible query identities
under the exact AccessKind names. `inventory.hbs` renders this model as a Markdown
CRUD/column matrix using ordinary nested iteration, and `inventory.md` is its
expected output. A CLI unit test converts the maintained TOML to JSON and checks
that rendering without SQL interpretation, ID lookup or data transformation.
Both this source-tree runner and installed-package validation compare the real
CLI inventory with the same TOML golden.
