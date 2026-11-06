# `sql-fun extract catalog`

Extract a PostgreSQL builtin catalog from an explicitly selected server or Docker
image. PostgreSQL client tools (`psql`) are required for both sources; the Docker
source also requires Docker CLI access to a running daemon.

```text
sql-fun extract catalog [OPTIONS] --output <OUTPUT>
```

| Option | Behavior |
| --- | --- |
| `--database-url <CONNECTION>` | libpq connection string or URL; can also be supplied through `SQL_FUN_DATABASE_URL`. The option takes precedence over the environment. Conflicts with `--docker-image`. |
| `--docker-image <IMAGE>` | PostgreSQL image reference; conflicts with the connection source. |
| `--docker-host-address <ADDRESS>` | Address used to reach the published port; default `127.0.0.1`. Requires `--docker-image`. |
| `--docker-bind-address <ADDRESS>` | Host interface on which the port is published; default `127.0.0.1`. Requires `--docker-image`. |
| `--definition-schema <NAME>` | Include additional collation and operator-class definitions from this exact schema. Repeatable. |
| `--output <PATH>` | Required new or empty output directory. |
| `--timeout-seconds <SECONDS>` | Extraction/readiness timeout; positive integer, default `120`. |
| `-h`, `--help` | Print help. |

Choose exactly one source. The environment variable also counts as a connection
source. To avoid credentials in shell history, prefer a password file or libpq
service settings. For example, with a configured PostgreSQL 17 server:

```sh
SQL_FUN_DATABASE_URL='host=db.example.org dbname=postgres user=catalog_reader' \
  sql-fun extract catalog --output ./work/postgres-17
```

Success exits with status 0; extraction and I/O failures exit nonzero, while
invalid CLI usage exits with status 2. See the [extraction guide](../extract.md)
for permissions, security, output publication, cleanup, and Docker examples. See
[common options](../README.md#common-options) for root-level options.
