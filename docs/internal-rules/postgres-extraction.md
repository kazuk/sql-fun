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
SQL-callable scope, excluding internal/trigger signatures. Application schema
SQL and extension scripts remain separate inputs. The type master additionally
includes persistent installed base types as described below.

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

## Function row properties

Each `functions.json` overload additionally records `kind` (`pg_proc.prokind`)
and `returns_set` (`pg_proc.proretset`). View-write analysis uses these source
facts to disqualify aggregate/window/set-returning projections without changing
the readable function-signature model. Regenerate older masters before using
function-containing writable views; missing properties remain explicitly
unimplemented at that provider boundary. Builtin view rendering additionally
retains reloptions, column defaults and
bootstrap trigger/rule declarations. Rule targets are explicitly qualified; the
extractor checks the supported server deparser prefix before replacing it. These
facts use the existing typed identifier/fragment rendering contract and are
limited to the already selected bootstrap views and bootstrap object OIDs.
The internal `declarations` rendering field carries canonical ALTER VIEW defaults,
CREATE TRIGGER, and CREATE RULE SQL into the ordinary schema loader. It replaces
the old `write_metadata` transport name; no parallel writable-view catalog is
constructed. These statements, reloptions, dependency ordering, and function row
properties remain necessary semantic inputs. The transport rename leaves emitted
schema SQL and committed PostgreSQL 17/18 artifacts unchanged.
Scope and format version remain unchanged. The installed extraction integration
harness verifies the fields
for scalar, aggregate and set-returning functions on both supported versions.

## Type categories

`pg_type.json` records required `schema`, `name` and `collatable` fields, along
with `category`. Qualified identity is the exact `(schema, name)` pair. For
defined types, `category` is
exactly `pg_type.typcategory`, a single printable ASCII code, including uncommon
builtin and user-defined codes. Shell types emit null because PostgreSQL does
not guarantee their classification. The existing bootstrap eligibility is
unchanged, including eligible undefined shells. In addition, normal extraction
unconditionally includes persistent installed base types (`typtype = 'b'`),
including extension/user types and normal-OID definitions in `pg_catalog`.
Exclude `pg_toast` and temporary/toast-temporary namespaces; identify generated
ordinary array rows by another type's `typarray` relationship and omit them from
this additional selection. Do not infer arrays from names or generic `typelem`.
Nominal vectors and custom element-bearing base types retain their identities.
The union emits each identity once: existing bootstrap OID order first, then
additional types ordered by exact namespace/name using `C` ordering. This does
not select application schema SQL, extension files or further catalog payloads.

The semantic type definition retains category independently of type names,
preferred status, and casts. `TypeDefinition::category` uses the supplied
catalog context to restrict PostgreSQL-specific semantics to that dialect.
PostgreSQL enum definitions have category `E`; composite and table-row
representations have `C`. DDL domains retain their resolved base category at
analysis time (including `A` for a declared array base). Missing base metadata
remains unavailable. Synthetic anonymous composites describe row structure and
have `C`; an unclassified nominal definition remains unavailable.

The category field remains optional on semantic definitions; this does not
provide compatibility with older catalog generations lacking required `schema`
or `collatable` keys. Missing/null category is distinct from `X` (the PostgreSQL
unknown type) and
`U` (user-defined). Malformed supplied codes fail loading with the catalog path.
New metadata is serialized without changing the existing nominal-reference shape;
category is an additive optional field. Domain classification is retained through
serialization, independently of the existing domain identity/equality contract.

Committed generations and the full extraction CLI support PostgreSQL 17/18.
Automated category load/read round-trips use these existing committed catalogs;
no separate oracle snapshots are maintained. PostgreSQL 15–18 behavior can be
checked with small development-oracle queries without extending the CLI's
supported version matrix or making ordinary Rust tests depend on live servers.

## Type collation capability

Each `pg_type.json` record includes `collatable`: a boolean from
`pg_type.typcollation <> 0` for defined types, or explicit null when
`typisdefined` is false. Do not exclude eligible shells or interpret their
unspecified properties as false. The key is mandatory even for null values;
missing keys and malformed values fail loading with the catalog path. `schema`
is also mandatory, without a `pg_catalog` default. Duplicate qualified type
identities and empty/NUL-containing identifiers fail loading. Regenerate the
bundled PostgreSQL 17/18 masters for the current format; no legacy reader exists.

The existing `BuiltinTypeInfo` / `BuiltinTypeDefinition` path retains
`collatable: Option<bool>` directly, preserving the existing flattened reference
shape. Serialized definitions keep the field, including explicit null. There is
no discriminator, separate registry, provenance flag or supplementary provider.
No raw OID or default-collation identity is needed for this capability fact.

`TypeReference::collation_capability` uses `ParseContext` and the existing type
references. Nominal definitions return their own retained fact; domains recurse
through their resolved base and ordinary arrays through their element. This
includes nested domains, domain/array combinations and owned dynamic definitions.
Vectors read their own fact rather than an arbitrary element's fact. Enum,
composite, table-row and anonymous-composite definitions are non-collatable.
Undefined facts, unresolved references, cyclic domain chains and non-PostgreSQL
contexts return `None`. The query reports no diagnostics and performs no
additional discovery. SQL consumers own any interpretation, declaration
validation, diagnostics and DDL application decisions.

This scope is unconditional in ordinary `extract catalog` and does not depend
on `--definition-schema`, add another option, or add initialization prompts.
That existing option continues to select only collation/operator-class masters.

## Additive master definitions

Format 1 additionally produces `collations.json` and `operator_classes.json`.
Existing payloads and metadata representations are unchanged. The production
semantic loader now requires both masters for PostgreSQL catalog
initialization, and installed-generation validation checks their presence and
JSON structure. The semantic loader validates composite identities, encoding,
default-collation provenance, and class/family method consistency before exposing
definitions through `BaseParseContext`. Missing masters fail with an artifact path;
they never become empty successful lookups. Format remains 1, following the
explicit #582 decision; older incomplete generations must be extracted again.
Standalone catalog import retains its existing producer-side contract. Additional
files travel with the
candidate through publication, extension import, and new initialization.

`--definition-schema` is repeatable and selects an exact namespace string, not
an SQL identifier expression or a search-path entry. Names are deduplicated and
sorted; empty/NUL-containing or nonexistent names fail before publication.
Explicit selection includes all master definitions in those namespaces,
including normal-OID user and extension definitions. Without this input, masters
include bootstrap definitions in `pg_catalog` only. Other payload scopes remain
unchanged. Nonempty selections add a sorted `definition_schemas` array to
`metadata.json`; the existing `scope` still describes builtin schema extraction.
No schema is inferred from the connection search path, extension files, or
application metadata. Referenced identities outside the selection are included
inline, without selecting further definitions.

### Collations

`collations.json` is an object with `database` and `collations` members.
`collations` is an array ordered by `(schema, name, encoding)`, using PostgreSQL's
`C` collation for exact UTF-8 string ordering. Each entry contains:

| Field | Representation |
| --- | --- |
| `schema`, `name` | Exact namespace/object identifier strings |
| `encoding` | PostgreSQL encoding integer; `-1` means any encoding |
| `provider` | PostgreSQL provider code, including `d` for database default |
| `deterministic` | Boolean from `collisdeterministic` |
| `collate`, `ctype`, `locale`, `icu_rules`, `version` | Nullable catalog strings |

Identity is `(schema, name, encoding)`. Preserve every encoding variant in scope;
extraction does not apply consumer encoding eligibility or name resolution.
`database` contains `encoding` (integer), `encoding_name` (string), `provider`,
`collate`, `ctype`, `locale`, `icu_rules`, and `version` from the selected source
database's encoding and locale configuration. Locale/rules/version fields retain
catalog nulls. Its `default_collation` is an object with `schema`, `name`, and
`encoding`, identifying the extracted provider-`d` collation. The database's name
is not an object identity and is not serialized. No effective collation is
chosen for an index or column here.

### Operator classes

`operator_classes.json` is an array ordered by `(schema, name, access_method)`
with `C` string ordering. Each entry contains:

| Field | Representation |
| --- | --- |
| `schema`, `name` | Exact namespace/object identifier strings |
| `access_method` | Global access-method identifier string |
| `family` | Object with `schema`, `name`, and `access_method` |
| `input_type` | Object with exact type `schema` and `name` |
| `default` | Boolean from `opcdefault` for this method/input type |
| `key_type` | Type identity object, or null when `opckeytype = 0` |

Class identity is `(schema, name, access_method)`; family identity also includes
its method. Type identities use catalog namespace/name, including arrays,
domains, and pseudo-types, rather than SQL display spellings. Joins ensure the
referenced family belongs to the same method and preserve type identities even
outside selected namespaces. Null `key_type` means no distinct storage type,
not a missing reference. Matching and default selection are consumer work.

Neither payload exports OIDs as portable identities. Database locale/provider,
server encoding/build, installed definitions and selected namespaces are inputs
to reproduction. Existing pinned-image CI reproduces both new files, alongside
all previously committed payloads and metadata. Producer-side offline checks
cover identity uniqueness/order, relationship shapes and default provenance;
real-server extraction checks cover explicit scope, legal unusual names,
encoding properties, cross-schema references and installed extension masters.
These checks do not claim production conflict-target lookup support.

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

## Initialization integration tests

The extraction script first exercises the actual interactive terminal flow with
connector-only, named and malformed project metadata and an ambient metadata
path. Bare init must ask for engines without inspecting those files. The tests
also cover explicit home confirmation, unselected engine candidates and SQLite
dispatch without PostgreSQL tools. It also exercises
PostgreSQL initialization from Docker and
an existing server, reruns, and source/version mismatch diagnostics. The
installed-CLI Docker initialization contract is a Scrut fixture. It verifies
the supported PostgreSQL target set, the CLI completion messages, and all
installed builtin metadata files. Run it with an explicit CLI executable and
Docker connectivity:

```sh
SQL_FUN_BIN=/path/to/sql-fun \
SQL_FUN_TEST_DOCKER_HOST=127.0.0.1 \
SQL_FUN_TEST_DOCKER_BIND=127.0.0.1 \
BASH_ENV="$PWD/cli/postgres-contracts/setup.sh" \
scrut test cli/postgres-contracts
```

This requires Scrut, `psql`, Docker CLI, and daemon access. Supply
`SQL_FUN_TEST_DOCKER_HOST` and `SQL_FUN_TEST_DOCKER_BIND` explicitly when
connectivity requires specific addresses. CI runs the fixture against the
Debian-installed package on the Docker-capable Ubuntu runner.

The source and installed SQLAST regression suites exercise committed builtin
schemas through the CLI analysis path. The separate Rust whole-schema bootstrap
and copied-generation replay tests were discarded in #828 because their
repeated schema loading exceeded the per-case budget. The retained
`extracted_artifacts.rs` checks load catalog metadata and validate PostgreSQL
syntax without repeating complete semantic schema initialization.

The generic initialization tests also verify explicit choices and dispatch:
SQLite-only setup does not run PostgreSQL setup, omitted choices are not filled
in even for a single configured datasource, and incomplete `--yes` requests fail
before home creation. PostgreSQL cases supply explicit engines/datasources,
source, versions and home parameters. A connected server constrains the version
choice; it is not permission to select that version automatically.
