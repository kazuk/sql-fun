# PostgreSQL extraction format 1

The installed CLI owns queries, templates, the administrator export script, and
bundle import. `extract catalog` is a source adapter around one PostgreSQL
psql extractor. `init` integration and broader semantic bootstrap are
separate work.

## Catalog contract

The loader payload consists of `schema.sql`, `pg_type.json`, `functions.json`,
`func_detail.json`, `binary_op.json`, `left_unary_op.json`, `right_unary_op.json`,
and `casts.json`. Existing loader field names are retained, including
`is_prefferd`. Metadata records the protocol version, extractor version, actual
`server_version_num`, source kind, and Docker image content ID when applicable.
Connection credentials and endpoint parameters are never recorded.

Queries run in a repeatable-read, read-only transaction with a fixed search path,
standard-conforming strings, UTC timezone, and a statement timeout. Format 1
covers released PostgreSQL 17 and 18. Bootstrap object OIDs are below PostgreSQL's
`FirstNormalObjectId` (16384); this boundary separates bootstrap catalogs from
objects installed later, even inside `pg_catalog`. System schema rendering covers
`pg_catalog` and `information_schema`; function metadata retains the loader's
SQL-callable scope, excluding internal/trigger signatures. Application schema and
installed extension catalogs are separate inputs.

SQL identifiers are tagged as identifiers and rendered only through `ident`.
Server-rendered expressions, types, argument declarations, and definitions are
tagged as fragments and rendered only through `sql`. A helper rejects the wrong
kind. The Handlebars default escape policy remains enabled, strict missing-field
checking is enabled, and templates do not use raw interpolation. SQL fragments
must originate in catalog deparsing; never tag arbitrary CLI input as SQL.

JSON object keys are sorted recursively. Queries explicitly order arrays;
argument order and view dependency order are meaningful and must not be sorted
away. PostgreSQL deparsing, collation, exact server build, and installation files
are extraction inputs. The pinned integration images define the official
reproduction environment. Verify both payload and metadata against committed
artifacts in the installed-binary CI job.

## Extension bundle contract

The standalone script writes an uncompressed GNU tar/PAX archive with:

- `VERSION`: two newline-terminated lines, format number `1` and PostgreSQL
  release `major.minor` from the selected `pg_config`;
- `extension/`: an optional directory entry;
- `extension/*.control` and `extension/*.sql`: regular files, preserving bytes.

Archive ordering, timestamps, owner/group IDs, modes, and PAX header names are
normalized. Export follows packaging symlinks into copied regular files. Export
must run against a stable installation; it is not a filesystem snapshot.

The importer streams regular-file contents without a fixed size ceiling, checks
logical-size arithmetic for overflow, requires an exact PostgreSQL release match,
and rejects missing/unsupported
versions, duplicate paths, traversal, unexpected paths/suffixes, symlink/hardlink
entries, and special files. It creates new files in a private staging directory
rather than invoking a general tar unpack operation. Compatibility checks do not
prove that identically numbered vendor installations are equivalent: selecting
the corresponding installation remains an administrator responsibility.

## Publication and resource ownership

An adjacent persistent standard-library file lock coordinates writers to the same
output. Only absent or empty real directories are accepted; nonempty outputs are
rejected before extraction and checked again before publication. Staging is on
the destination filesystem. Publication creates directories and exclusively creates
files, synchronizing written files and the destination directory. It never deletes,
exchanges, or overwrites existing entries. An I/O failure or crash during the final
copy may leave a partial candidate; this operation is not an installed-generation
transaction. Installation/replacement is outside extraction's responsibility.
Import reads `--catalog` and stages its catalog files with the new bundle before
writing a separate candidate. Input extension files are not copied. Failed
validation never publishes partial output. A crash can leave a staging directory.

Both source adapters invoke host `psql` with a libpq connection string.
Use libpq password files rather than connection-string passwords. A single
session owns the read-only,
repeatable-read transaction and all fixed session settings. `-X` disables startup
scripts, `ON_ERROR_STOP` aborts on query errors, and password prompting is disabled.
Libpq owns authentication, transport, and certificate configuration. Diagnostics
omit subprocess server messages because they may contain credentials. Docker
extension export streams its archive directly to a staging file.
Docker uses a random task name, records the created ID, assigns an ephemeral
published port, and removes only that ID and its anonymous volumes. Each Docker
operation is bounded. Handled interrupts during provisioning wait for the bounded
creation result so a created container can be removed; extraction interrupts and
timeouts also run cleanup before returning. Uncatchable termination or a daemon
failure cannot guarantee cleanup. Diagnostics identify the task resource for
manual recovery without any global prune or fixed-name deletion.

## Validation

`test-postgres-extraction` exercises Docker and independently supplied connection
paths against pinned real servers. The supplied-connection process has an invalid
Docker endpoint to prove it does not need Docker access. Tests cover payload
identity, deterministic export, restricted database roles, user/extension object
isolation, invalid connections/bundles, failure preservation, and timeout cleanup.
Rust tests validate quoting and SQL syntax, adversarial archive entries,
publication isolation, and existing catalog/extension loaders.
