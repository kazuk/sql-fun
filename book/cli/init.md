# Initialize development support

`sql-fun init` is the interactive entry point for initializing engine-specific
development support. It asks for choices that you have not supplied explicitly,
even when only one option is available. Bare initialization does not inspect
Cargo.toml or discover project metadata.

```console
sql-fun init
```

First choose the sql-fun home directory, then explicitly choose engines.
Use `--engines postgres`, `--engines sqlite`, or `--engines postgres,sqlite`
to supply the engine choice directly.

Project-aware initialization requires an explicit request: use the init option
`--datasource <name>` or the common CLI option `--metadata-file <path>`.
For example, `sql-fun --metadata-file Cargo.toml init` reads that file and asks
which named datasources to initialize. Metadata provides candidates; it does not
select them. Fields such as `connector` are not datasource candidates. If the
requested project has no named datasources, choose engines instead.
`--datasource` requests project metadata discovery when no file is supplied.
An ambient `SQL_FUN_METADATA_FILE` alone does not request project access.
Do not combine `--engines` with `--datasource`.

You also choose the sql-fun home directory. The interactive prompt suggests
`~/.sql-fun` or the configured common home; accepting the suggestion is an
explicit choice. Use `init --sql-fun-home <path>` to supply the choice directly.
The prompt, `--sql-fun-home`, and `SQL_FUN_HOME` accept absolute paths, paths
relative to the command's current working directory (such as `./work`), and
paths beginning with `~/` (such as `~/work`). Interactive setup shows the
resolved absolute path for confirmation before using or creating that directory.
Declining returns to path editing. A supplied `--sql-fun-home` skips this
confirmation under `--yes`; `SQL_FUN_HOME` alone does not satisfy `--yes`.
Application schemas and migrations remain managed by your own tools.

## Automatic setup

`--yes` completes setup without prompts only when all required choices are
supplied through CLI parameters. It does not choose omitted values for you.
An incomplete request reports a missing parameter before setup begins.

For PostgreSQL, supply an engine or datasource selection, the home directory,
the source (`--docker` or `--database-url`), and `--engine-versions`:

```sh
sql-fun init --yes --engines postgres --sql-fun-home "$HOME/.sql-fun" \
  --docker --engine-versions 17,18
```

For a configured datasource:

```sh
sql-fun init --datasource main --yes --sql-fun-home "$HOME/.sql-fun" \
  --docker --engine-versions 17
```

SQLite has its own setup path. Its development-support setup is still under
development; selecting it currently reports that status. This does not imply
that SQLite will never require initialization. PostgreSQL tooling is not
required when only SQLite is selected.

## PostgreSQL setup

PostgreSQL semantic analysis requires builtin metadata matching the target
version. PostgreSQL setup installs builtin types, functions, operators, casts,
and catalog objects separately from your application schema dump.

Choose a source and a nonempty set of supported versions. Currently extraction
supports PostgreSQL 17 and 18. Multiple selected datasources share installations
for the same version; their configured versions must be included in your
explicit target set. Unselected datasources do not add initialization tasks.

### Docker source

Install PostgreSQL client tools (`psql`) and Docker CLI, and provide access to a
Docker daemon. Choose Docker interactively or supply `--docker` explicitly.
No source or version is selected automatically.

Each version uses its registered PostgreSQL image and a temporary container.
The extractor removes its own containers and volumes after completion or a
handled interruption. Image retrieval requires network access unless the image
is already present.

If sql-fun runs inside a container or uses a remote Docker host, use
`--docker-host-address` for the reachable host address and
`--docker-bind-address` for the host interface publishing PostgreSQL ports.
Both connectivity settings default to `127.0.0.1`. See [extraction](extract.md)
for connectivity and resource-cleanup details.

### Existing-server source

This source requires `psql`, network access, and appropriate PostgreSQL catalog
permissions. It does not require Docker. Supply `--database-url` with a libpq
connection string or URL, or choose this source and enter the connection string
interactively. Prefer a password file to inline credentials:

```sh
sql-fun init --yes --engines postgres --sql-fun-home "$HOME/.sql-fun" \
  --database-url 'host=db.example.org user=catalog_reader sslmode=verify-full' \
  --engine-versions 17
```

Detection determines the connected server's actual version and constrains the
available target. It does not replace an explicit choice: the interactive flow
asks you to select the detected version when omitted; `--yes` requires
`--engine-versions`. A supplied version set must contain only the detected
version. Unsupported versions produce an error. `--docker` conflicts with
`--database-url`. The extraction command's `SQL_FUN_DATABASE_URL` environment
variable does not select an init source.

Existing-server initialization installs builtin metadata only; it does not copy
extension files. The [extraction documentation](extract.md) describes
administrator-produced extension bundles separately.

### Installation and reruns

Metadata is installed under `<chosen-home>/postgres/<version>/`. Configure
subsequent check/server commands with the same home (normally `~/.sql-fun`).

A complete compatible installation is preserved on rerun. An incomplete or
incompatible installation is an error: move the affected version directory
aside before retrying. `--yes` does not overwrite metadata. Only one initializer
may install a given version at a time.

Versions are installed independently. If a later version fails, completed
versions remain usable and the command reports failure. Correct the reported
problem and rerun initialization. `--timeout-seconds` sets the extraction timeout
for each version (default 120).

Release artifact downloading and extension filtering are not implemented by
this command. The previously planned `--github-repo`, `--sql-fun-version`,
`--postgres-extensions`, and `--docker-image-tag` options are not accepted.
