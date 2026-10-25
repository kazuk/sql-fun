# Extract PostgreSQL metadata

Use `sql-fun extract` to obtain builtin catalogs for PostgreSQL 17 or 18.
It writes `schema.sql`, catalog JSON files, and `metadata.json` to the directory
you specify. Outputs include `collations.json` and `operator_classes.json` for
collation and index operator-class definitions. Docker extraction also exports
and imports the server installation's
extension SQL and control files. Extracted builtin views retain their options,
column defaults and write rules/triggers. Function metadata distinguishes scalar,
aggregate/window and set-returning functions so view-write analysis can reject
row-expanding projections. Refresh older builtin generations before relying on
these view-write capabilities. Application views and their declarations still
come from your explicitly selected schema inputs.

These commands produce candidate generations in a working directory.
`$SQL_FUN_HOME` contains installed state; it is not an extraction workspace.
Installing or upgrading a generation is a separate operation.
Install the PostgreSQL client tools (`psql`) on the machine running sql-fun
for both connection sources.

## From a Docker image

Install Docker CLI and provide access to a Docker daemon:

```sh
sql-fun extract catalog --docker-image postgres:17 \
  --output ./work/postgres-17
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
`psql`/libpq. Prefer the environment variable to putting credentials in
command history. The string is passed to the `psql` subprocess; keep passwords
out of the string and use libpq password files or a service definition:

```sh
export SQL_FUN_DATABASE_URL='host=db.example.org user=catalog_reader'
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL dbname=postgres"
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL sslmode=verify-full"
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL sslrootcert=/path/to/ca.pem"
sql-fun extract catalog --output ./work/postgres-17
unset SQL_FUN_DATABASE_URL
```

The account needs connection and catalog-read permissions. Extraction performs
read-only queries; it does not create extensions or change the source database.
Authentication, TLS, CA certificates, password files, and service definitions use
[libpq connection settings](https://www.postgresql.org/docs/17/libpq-ssl.html).
Use `sslmode=verify-full` and the appropriate `sslrootcert` to verify both the CA
and server hostname. `sslmode=require` requires encryption but does not by itself
guarantee hostname verification. `sslmode=disable` explicitly permits unencrypted
connections. Password prompts are disabled; configure credentials before running
extraction. This source needs `psql`, but does not need Docker.

`--timeout-seconds` bounds readiness and extraction (default: 120 seconds).
Docker image retrieval/creation and cleanup have separate bounded operations.
Connection diagnostics omit credentials and server-provided error text.

Builtin metadata covers bootstrap objects in `pg_catalog` and
`information_schema`. User-created objects and installed extension objects are
not added to builtin catalogs. Export your application schema separately using
your normal schema workflow.

## Include additional master definitions

To include collations, operator classes, and range type families from additional
schemas, select each schema explicitly. Pass its exact name as one argument,
without SQL identifier quoting inside the value:

```sh
sql-fun extract catalog --database-url 'host=db.example.org dbname=app' \
  --definition-schema public --definition-schema 'Extension Schema' \
  --output ./work/app-masters
```

Repeated names are deduplicated. A nonexistent or empty name is an error.
Selection includes user-created and installed extension definitions in those
schemas, together with their referenced type/family identities. It does not
export application tables or install extensions. Selected range and multirange
identities are added to `pg_type.json` together with their actual subtype and
paired range/multirange relationships. Names need no particular suffix.
Omitting this option retains builtin-only extraction. The selected scope is
recorded in `metadata.json`.

Range relationships and the master files are additive to catalog format 1.
Older type catalogs without range relationships remain readable, but cannot
supply range polymorphic inference; extract a new catalog to use that behavior.

Existing catalog import and initialization continue working, including with
older format-1 generations
that lack these files. Production semantic loading of these masters is separate
work; extracting them alone does not enable conflict-target analysis.

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
  --catalog ./work/postgres-17 --output ./work/postgres-17-with-extensions
```

The bundle's PostgreSQL major and minor release must exactly match the extracted
catalog. The administrator must select the same PostgreSQL installation used by
the server, particularly when several installations or vendor builds coexist.
Version matching cannot prove two vendor builds have identical extension files.
The importer rejects unsupported formats, duplicate paths, links, special files,
and unsafe paths. There is no fixed archive-size ceiling; provide enough disk
space for the bundle, staging files, and candidate output. Importing copies files;
it does not execute extension SQL or install extensions in PostgreSQL.

When configuring an extension for subsequent SQL analysis, specify its installation
version explicitly (for example, `uuid-ossp@1.1`). Automatic version inference
currently also counts upgrade scripts; this separate loader limitation is tracked
in [#270](https://github.com/kazuk/sql-fun/issues/270).

## Candidate output and failures

For both extraction and import, `--output` must be absent or an empty real
directory. Nonempty directories, files, and symlinks are rejected without
modifying their contents. Import reads `--catalog` without modifying it and
copies its catalog files with the new bundle into the separate output candidate.
It does not retain extensions from the input catalog.

Extraction and bundle validation complete in a private staging directory before
writing the candidate. Validation failures leave the output unchanged. An I/O
failure or forced termination during the final copy can leave a partial candidate;
inspect it and choose a new empty output for retry. Existing files are never
overwritten. These commands do not install, merge, delete, or exchange installed
generations.

Concurrent sql-fun writers to the same output are rejected. Keep the adjacent
`.sql-fun.lock` file; deleting it can defeat writer coordination. Avoid modifying
the output while extraction is running. No atomic directory-exchange support is
required.
