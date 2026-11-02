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

## Reviewing answers and confirming setup

Press **Esc** to return to the preceding question. At the first question, Esc
leaves the wizard open. Previous answers are restored for editing; press Enter
to reconfirm an unchanged answer. Changing a selection clears affected dependent
answers. For example, switching PostgreSQL sources clears the source-specific
connection and version choices, and changing a connection requires confirming
the server version again. Unrelated answers remain available.

After all answers have been validated, explicitly select **Ok** or **Cancel**
with Space, then press Enter. Neither action is selected initially or when you
return to this confirmation. Enter alone does not start setup or cancel it.
Esc from this confirmation returns to the preceding question.

The question phase may read explicitly selected configuration and connect to a
selected PostgreSQL server to validate its version. It does not create the home
directory or install metadata. Home creation and engine provisioning begin only
after explicit final Ok. Press **Ctrl+C** at a question, or select final Cancel,
to leave without changing files. Cancellation prints a short message and exits
with status 1. Existing user data is never deleted as cancellation cleanup.
Installation errors after Ok may leave a created home and completed engine
versions in place; cancellation before Ok and installation failure are distinct.

## Diagnostic theme

Successful initialization requires a valid diagnostic theme at
`<chosen-home>/highlighter/default.toml`. A valid existing theme, including your
customizations, is preserved without downloading. The theme is validated using
the diagnostic renderer's color rules; invalid TOML or colors are errors.

For a missing theme, explicitly choose **Download default theme** or
**Create/Edit theme**. Downloading requires `curl` (preferred) or `wget` with TLS
support and trusted certificates on `PATH`. The default is downloaded from the
[published manual](https://kazuk.github.io/sql-fun/highlighter/default.toml).
Downloads use bounded timeouts and fail on TLS, HTTP, process or validation errors.

For creation or correction, select Create/Edit and supply an editor executable
name or path (without shell arguments). The editor opens a temporary working
copy after final Ok. It must finish successfully and produce a valid theme.
An invalid existing theme is replaced only through this explicitly authorized
edit; directories, symlinks and unsupported destinations are rejected.
There is no Skip option. On failure, init reports failure and preserves existing
user data. Correct the reported problem and rerun.

The final review shows the action, path, and download URL or editor. No theme is
downloaded, staged, edited or installed before final Ok. Back retains the chosen
action for editing; changing home requires checking the new destination.

## Automatic setup

`--yes` completes setup without prompts only when all required choices are
supplied through CLI parameters. It does not choose omitted values for you.
An incomplete request reports a missing parameter before setup begins.
A valid existing theme is retained. If the theme is missing, explicitly pass
`--install-default-theme` to authorize downloading it. This option never
overwrites an existing theme. Correct invalid existing themes before using
`--yes`; it cannot choose an editor or a download on your behalf.

For PostgreSQL, supply an engine or datasource selection, the home directory,
the source (`--docker` or `--database-url`), and `--engine-versions`:

```sh
sql-fun init --yes --install-default-theme --engines postgres \
  --sql-fun-home "$HOME/.sql-fun" \
  --docker --engine-versions 17,18
```

For a configured datasource:

```sh
sql-fun init --datasource main --yes --install-default-theme \
  --sql-fun-home "$HOME/.sql-fun" \
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
sql-fun init --yes --install-default-theme --engines postgres \
  --sql-fun-home "$HOME/.sql-fun" \
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
aside before retrying. `--yes` does not overwrite metadata. Concurrent initializers
preserve the first complete compatible installation published for that version.
Readers see the version directory only after its metadata has been validated.

A hard crash can leave a `<version>.installing.<unique>` staging directory under
`<chosen-home>/postgres/`. After confirming its initializer is no longer running,
you can remove that leftover directory before retrying. Other staging directories
are not cleaned up automatically.

Versions are installed independently. If a later version fails, completed
versions remain usable and the command reports failure. Correct the reported
problem and rerun initialization. `--timeout-seconds` sets the extraction timeout
for each version (default 120).

Release artifact downloading and extension filtering are not implemented by
this command. The previously planned `--github-repo`, `--sql-fun-version`,
`--postgres-extensions`, and `--docker-image-tag` options are not accepted.
