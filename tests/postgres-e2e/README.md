# PostgreSQL execution E2E

For contributors testing the complete public SQL workflow, run from the
repository on a Linux host with Docker Engine access:

```sh
tests/postgres-e2e/run.sh --local
```

The command builds the local CLI/server, starts disposable PostgreSQL 18,
applies `schema.sql` with `psql`, generates a fresh lockfile with `sql-fun check`,
and executes the ordinary Cargo consumer. Rust assertions verify bound integer
and text values through `INSERT`, `SELECT`, `UPDATE`, and `DELETE`. Failure in
any phase makes the command fail. Containers, server processes, generated
lockfiles, and temporary state are cleaned up on exit or handled interruption.
Cargo artifacts persist in the repository's `target/` (or an explicitly supplied
`CARGO_TARGET_DIR`) and are reused across queries and subsequent runs.

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

The runner copies the repository's PostgreSQL 18 builtin catalog into a
temporary `SQL_FUN_HOME`. As a temporary workaround for [#718](https://github.com/kazuk/sql-fun/issues/718),
it also copies the existing default highlighter theme to the path expected by
`check`. Remove that provisioning when #718 is fixed.
