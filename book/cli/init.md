# Initialize PostgreSQL support

`sql-fun init` installs the builtin metadata required for PostgreSQL semantic
analysis. This includes builtin types, functions, operators, casts, and catalog
objects. Metadata must match the PostgreSQL version configured for your datasource.
It is separate from your application schema dump; initialization does not manage
schemas or migrations.

## From Docker

Install PostgreSQL client tools (`psql`) and Docker CLI, and provide access to a
Docker daemon. Docker is the default initialization source:

```console
sql-fun init
```

Select one or more supported PostgreSQL majors in the interactive prompt.
Currently, builtin extraction supports PostgreSQL 17 and 18. You can select any
non-empty combination. Non-interactive setup defaults to all supported stable
versions:

```console
sql-fun init --yes
sql-fun init --yes --engine-versions 17,18
```

Each version uses its registered PostgreSQL image and a temporary container.
The extractor removes its own containers and volumes after completion or a
handled interruption. Image retrieval requires network access unless the image
is already present.

If sql-fun runs inside a container or uses a remote Docker host, use
`--docker-host-address` for the reachable host address and
`--docker-bind-address` for the host interface publishing PostgreSQL ports.
Both default to `127.0.0.1`. See [extraction](extract.md) for connectivity and
resource-cleanup details.

## From an existing server

This source requires `psql`, network access, and appropriate PostgreSQL catalog
permissions. It does not require Docker. Use libpq authentication/TLS settings;
keep passwords in a password file rather than in command arguments:

```sh
export SQL_FUN_DATABASE_URL='host=db.example.org dbname=postgres'
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL user=catalog_reader"
SQL_FUN_DATABASE_URL="$SQL_FUN_DATABASE_URL sslmode=verify-full"
sql-fun init --yes
```

Alternatively, supply `--database-url` with a libpq connection string or URL.
Initialization detects the connected server's major and installs that version.
Unsupported versions produce an error. If you supply `--engine-versions`, it must
select only the detected version. `--docker` explicitly selects Docker and
conflicts with an existing-server source.

Existing-server initialization installs builtin metadata only; it does not
copy the server's extension files. The [extraction documentation](extract.md)
describes administrator-produced extension bundles separately.

## Installation and reruns

Metadata is installed under `$SQL_FUN_HOME/postgres/<version>/`, with a default
home of `~/.sql-fun`. Use `--sql-fun-home` to choose another home, and pass the
same home to subsequent check/server commands. A configured datasource version
must be included in the selected target set. Standalone home initialization
also works outside a project.

If project metadata has no `default` datasource, select a configured name with
the global option, for example `sql-fun --datasource main init --yes`.

A complete compatible installation is preserved on rerun. An incomplete or
incompatible installation is an error: move the affected version directory
aside before retrying. `--yes` disables prompts; it does not overwrite metadata.
Only one initializer may install a given version at a time.

Versions are installed independently. If a later version fails, completed
versions remain usable and the command reports failure. Correct the reported
problem and rerun initialization. `--timeout-seconds` sets the extraction timeout
for each version (default 120).

Release artifact downloading and extension filtering are not implemented by
this command. The previously planned `--github-repo`, `--sql-fun-version`,
`--postgres-extensions`, and `--docker-image-tag` options are not accepted.
