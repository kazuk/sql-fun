# Extract PostgreSQL metadata

Use `sql-fun extract` to obtain builtin catalogs for PostgreSQL 17 or 18.
It writes `schema.sql`, catalog JSON files, and `metadata.json` to the directory
you specify. Docker extraction also exports and imports the server installation's
extension SQL and control files.

## From a Docker image

Install Docker CLI and provide access to a Docker daemon:

```sh
sql-fun extract catalog --docker-image postgres:17 \
  --output "$SQL_FUN_HOME/postgres/17"
```

Use an image digest instead of a mutable tag when repeatability matters.
The command creates a temporary PostgreSQL container and publishes a dynamically
allocated port on the Docker host's loopback address. It removes only its own
container and anonymous volumes, including after extraction failure or a handled
interrupt. A forcibly killed process or unavailable Docker daemon can require
manual cleanup of the task-owned container identified in diagnostics.

When running the CLI inside another container, the Docker host's loopback address
may not be reachable. Set `--docker-host-address` to the reachable host address
and `--docker-bind-address` to a specific interface on that host. These options
do not attach to or reconfigure existing containers. Docker access is needed only
for this source; the normal development container does not need permanent socket
access.

## From an existing server

Supply a PostgreSQL connection URL or connection parameters understood by
`tokio-postgres`. Prefer the environment variable to putting credentials in
command history:

```sh
export SQL_FUN_DATABASE_URL='host=db.example.org user=catalog_reader'
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL dbname=postgres sslmode=require"
sql-fun extract catalog --output "$SQL_FUN_HOME/postgres/17"
unset SQL_FUN_DATABASE_URL
```

The account needs connection and catalog-read permissions. Extraction performs
read-only queries; it does not create extensions or change the source database.
TLS uses the platform certificate store and validates the server certificate and
hostname. Install your organization's CA into that store when necessary.
`sslmode=disable` explicitly permits unencrypted connections; `sslmode=prefer`
may fall back to an unencrypted connection. Use `require` for remote connections
that must use TLS. Unsupported connection options are reported as configuration
errors. Neither `psql` nor Docker is needed for this operation.

`--timeout-seconds` bounds readiness and extraction (default: 120 seconds).
Docker image retrieval/creation and cleanup have separate bounded operations.
Connection diagnostics omit credentials and server-provided error text.

Builtin metadata covers bootstrap objects in `pg_catalog` and
`information_schema`. User-created objects and installed extension objects are
not added to builtin catalogs. Export your application schema separately using
your normal schema workflow.

## Obtain extension files from an administrator

A database connection cannot read the server's extension files. Obtain the
standalone export script and provide it to the server administrator:

```sh
sql-fun extract export-script > export-extensions.sh
```

The administrator can review and run it on the PostgreSQL host, without installing
sql-fun or granting SSH access to sql-fun:

```sh
PG_CONFIG=/path/to/postgresql/bin/pg_config \
  sh export-extensions.sh > extensions.tar
```

The script requires POSIX `sh`, GNU `tar`, `mktemp`, `cp`, `mkdir`, `rm`, the
matching installation's `pg_config`, and read access to its extension files.
It reads `pg_config --sharedir`, copies `*.control` and `*.sql`, and follows
packaging symlinks to copy their file contents. A dangling link is an error.
Ensure the files are not being updated while exporting. The archive is an
uncompressed tar bundle. Transfer it using your organization's normal mechanism,
then import it locally:

```sh
sql-fun extract import-extensions --bundle extensions.tar \
  --output "$SQL_FUN_HOME/postgres/17"
```

The bundle's PostgreSQL major and minor release must exactly match the extracted
catalog. The administrator must select the same PostgreSQL installation used by
the server, particularly when several installations or vendor builds coexist.
Version matching cannot prove two vendor builds have identical extension files.
The importer rejects unsupported formats, duplicate paths, links, special files,
unsafe paths, and bundles larger than 256 MiB. Importing copies files; it does
not execute extension SQL or install extensions in PostgreSQL.

When configuring an extension for subsequent SQL analysis, specify its installation
version explicitly (for example, `uuid-ossp@1.1`). Automatic version inference
currently also counts upgrade scripts; this separate loader limitation is tracked
in [#270](https://github.com/kazuk/sql-fun/issues/270).

## Replace or recover output

Successful extraction replaces the complete output directory. Connection-only
extraction does not retain extension files from a previous extraction; import a
matching bundle afterward. Extension import retains the existing catalog files
and replaces the complete `extension/` directory.

Failures before publication preserve the previous output. Concurrent writers to
the same output are rejected. Output must be a real directory, not a symlink.
Replacing an existing directory atomically currently requires Linux and a
filesystem supporting rename exchange. If unsupported, choose a new output
path. Keep the adjacent `.sql-fun.lock` file; deleting it can defeat writer
coordination. The temporary and final output reside on the same filesystem.
