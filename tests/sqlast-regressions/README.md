# SQLAST semantic regression contracts

This internal suite migrates SQL-observable assertions from builtin-dependent
and schema-dependent SQLAST Rust tests to the real CLI → client → server
analysis
path. The manifest is shared by source verification (`check-test-flow` workspace
step) and installed verification (`postgres-extraction`, after initialization
and catalog comparison).
No regression SQL is executed against PostgreSQL.

Source CI runs this suite in dedicated default-feature and all-features jobs,
using the corresponding CLI/server build artifacts. They do not wait for the
nextest builds or Rust tests. Internally, `check-test-flow --step=workspace`
selects `--workspace-phase=rust-test` for archived Rust tests and
`--workspace-phase=sqlast-test` for regressions with explicit `SQL_FUN_BIN` and
`SQL_FUN_TEST_SERVER_BIN` inputs. Both execution phases reuse artifacts without
compilation. The existing combined `--workspace-phase=test` route and normal
local full-check behavior remain supported.

## Committed CI scheduling

The local workspace check runs the suite serially and records full CLI command
wall times in `target/sqlast-regressions/timings.tsv`. After every group and the
negative controls succeed, `check-test-flow` regenerates `priorities.tsv`.
Include changes to that tracked file with the PR; the existing check command
also covers newly added groups. Failed or incomplete runs do not replace it.

The file contains only a priority tier and fixture/version group name. The ten
longest measured groups are `heavy`; the remaining groups are divided nearly
in half into `medium` and `light` (the extra group goes to medium for odd sizes).
For inventories smaller than ten, every group is heavy. Duration ties are
resolved by group name. Each tier is name-sorted: changes in timing order inside
a tier produce no Git diff. Timings, timestamps and numeric ranks are not stored
in the committed file. The current 146 groups are split 10/68/68.

CI's archived execution phases validate that the committed tiers exactly cover
the manifest, without duplicates or unknown groups, and have canonical ordering
and sizes. They then run one shared-server suite with **two workers**, consuming
heavy, medium and light in order. There is no calibration, duration estimation,
or priority regeneration in CI. Missing or stale classification is an error.
Default and all-features jobs retain independent binary producers and gates.

CI evidence is retained under fresh `run-*` subdirectories in
`target/sqlast-regressions/`, including the selected priorities, environment,
original inventory, worker reports, combined XML, claims, summaries and server
logs. Repeating an execution phase preserves earlier evidence. The direct
`run.sh` entry point used for installed-distribution validation remains serial.

`manifest.tsv` explicitly selects each fixture, schema level, optional project
schema, expected case count and PostgreSQL versions. Each JSON fixture is an
array of self-contained cases with `name`, `sql` and `expected` fields. Keep
each
SQL input and its expectation in the same object when adding, deleting or
reordering cases. Names preserve original Rust case identities and must be
nonempty and unique within the fixture. The runner validates manifest case
counts and passes fixture files directly to `sql-fun test`. The CLI validates
case objects and names before analysis.
Console/JUnit identities include the fixture path and case name; SQL diagnostic
context also includes the array index.
Expectations use the existing `sql-fun test` contract: complete diagnostic sets
and exact ordered columns, including absent types and unknown nullability.

The internal runner requires `SQL_FUN_HOME`, `SQL_FUN_BIN`,
`SQL_FUN_TEST_SERVER_BIN` and `SQL_FUN_REGRESSION_REPORT_DIR`. Its caller owns
catalog provisioning and executable selection. It starts and cleans up only its
own analysis server. It preserves logs and JUnit reports in the supplied report
directory, reports executed counts, and fails if any fixture fails or counts do
not match. Inputs within each fixture are independent and share a schema base;
setup DDL belongs in the schema fixture, not preceding query-array entries.

The pilot asserts success and an expected diagnostic. Negative controls
introduce
wrong result-column and diagnostic expectations, verify exit status 1 and
readable
expected/actual differences, and confirm a later case still passes. An invalid
selector and an unconfigured datasource must also fail. Expected-failure
evidence
lives under `controls/` so it can be inspected without treating it as a failing
regression JUnit suite.

The inventory documents migrated cases and internal assertions retained in Rust.
AST structure, binding identity, parameter domains, internal error contracts and
schema-layer state transitions stay at their existing unit/integration boundary.

The original #757 migration covered builtin-only cases. The #764 follow-up
adds default/project-schema consumers.
[#761](https://github.com/kazuk/sql-fun/pull/761)
resolved the earlier catalog-initialization problem. No catalog substitution or
diagnostic suppression is used.

`default` loads the actual installation's bundled `schema.sql`. `project` adds
the explicitly selected schema dump; passing `--schema-file` with `default`
does not load that project dump. Custom DDL lives in the manifest-selected SQL
files, while shipped information_schema consumers use the default layer.
Conflicting function declarations have separate dumps and case groups. The
runner's explicit search path remains `public`; cases requiring another search
path, database identity, or intermediate catalog states stay in Rust.

The [schema-dependent inventory](inventory-schema.md) maps public assertions and
retained internal coverage, including non-equivalent synthetic contexts. The
manifest totals are 1214 inputs for PostgreSQL 17 and 1197 for
PostgreSQL 18 (2411 total). These count positive contract inputs separately from
the three negative-control inputs, invalid-selector check and missing-datasource
check per version.

See [the broad inventory](inventory-retained.md) and the detailed
[ARRAY/min-max](inventory-arrays.md), [CASE/COALESCE](inventory-expressions.md),
[query/type/view](inventory-query.md), [cast](inventory-casts.md),
[vector](inventory-vectors.md), and [ARRAY-query](inventory-array-subqueries.md)
mappings for preserved scenarios and reasons that
internal assertions remain in Rust. Counts refer to SQL inputs, not removed Rust
tests: one old test may check multiple SQL expressions, and mixed tests retain
internal assertions after their observable result assertions move to the CLI.

The runner prints each manifest group's result, executed/expected case counts
and wall-clock seconds measured with Bash `time` around the actual `sql-fun
test`
command. The final summary aggregates those command durations and reports total
executed cases and cases/second. Build, server startup, catalog preparation,
report processing/upload and negative controls are excluded
from this timing. These values provide a baseline in source and installed CI
logs; timing variation does not affect success and no performance threshold is
enforced.

Issue #768 corrects the two DO NOTHING rule ON CONFLICT expectations in
`schema-views-precedence.json`. The project-schema `view-conflict-rules.json`
fixture adds six adjacent boundaries: action-bearing unconditional/conditional
INSTEAD rules, mixed INSERT rules, UPDATE-rule restrictions, and an ALSO NOTHING
rule with a trigger. Both fixtures use explicitly selected schema dumps on
PostgreSQL 17/18 through the shared source and installed regression paths.
Oracle probes used EXPLAIN only in rolled-back transactions on PostgreSQL 17.6
and 18.1. Rust retains direct support, destination binding, structured diagnostic
and catalog-state checks; see [the view inventory](inventory-schema-views.md).

Issue #765 restores implicit names for ordinary function calls and scalar
subqueries. [`output-names.json`](output-names.json) covers builtin output names,
alias precedence, nested queries, derived/CTE consumers, and parameter inference
on PostgreSQL 17/18. The schema-qualified and DML fixtures also assert corrected
correlated and RETURNING names. Existing ordinary-call expectations in the
metadata, array/vector consumer, domain-overload, composite and record fixtures
now retain the unqualified function name, including diagnostic-bearing calls.
These cases use the
shared source and installed paths; parameter diagnostics retain their existing
policy (tracked separately in #766).

[Issue #766 evidence](inventory-parameter-bindings.md) records unresolved
application input bindings as errors. The
`parameter-bindings` fixture checks native and application placeholders,
explicit input hints, known result columns with unresolved inputs, unrestricted
and conflicting inputs, array-index assignment coercion, repeated placeholders
and a later independent input. Both PostgreSQL 17/18 catalog installations run
these contracts through the shared source and installed paths. Internal tests
retain the distinction between empty, singleton, non-singleton and unrestricted
domains; server and CLI tests check completion, severity and exit behavior.

Issue #487 truth-test contracts live in `truth-tests.json`, selected with the
small `truth-tests.sql` project schema on PostgreSQL 17/18. The 101 cases check
all six predicates over boolean/NULL operands, literal spellings, nullable and
required columns, domains, invalid inputs, failed bindings, and deferred
parameter interactions. Diagnostics and exact ordered result-column names,
types and nullability run through both source and installed CLI checks. Rust
retains operand structure/coercions, serialization, binding outcomes and
admissible parameter domains; synthetic cast contexts remain unit tests because
they model conversion policies absent from the shipped catalog.

Issue #502 symmetric range predicates live in `between-symmetric.json`, selected
with `between-symmetric.sql` on PostgreSQL 17/18. Both variants cover endpoint
order, equal bounds, mixed comparable types, NULL positions, column lookup,
parameters, nested expressions, and WHERE consumers. The shared source and
installed paths check diagnostics and exact result metadata. Rust tests retain
comparison bindings, operand lookup outcomes, serialization, and correlated
parameter assignments; synthetic overload tests verify independent negative-form
operators and bounds that cannot be compared with each other.

Issue #499 adds the `distinctness` project-schema fixture for PostgreSQL 17/18
through the shared source and installed paths. It covers both null-safe
predicates, bare versus typed NULL, scalar/array/explicit-row operands,
parameter bindings and invalid comparisons. Rust retains binding, coercion,
serialization, correlated domains and SQLite feature contracts; see
[the distinctness inventory](inventory-distinctness.md).

Issue #802 adds `bounded-parameter-inference.json` with 49 project-schema
cases per PostgreSQL 17/18 catalog. The shared source and installed paths check
both distinctness predicates, ambiguous default candidates, explicit scalar,
binary and custom input types, SQL-established array types, SQL-established
non-default types, and repeated parameters constrained by another occurrence in
either order.
Rust tests retain default membership, correlated explicit assignment inspection,
resumption after an intermediate solve, array-only unresolved inputs, and
invariance under unrelated catalog growth. Additional cases cover late datetime
constraints, unresolved expression results that require unsupported catalog-wide
inference, later array/RETURNING context, and analysis recovery. Comparisons
use the existing 55-case `distinctness` group without timing pass/fail thresholds.

Table rename contracts for #527 live in `table-renames.json`, selected with
`table-renames.sql` on PostgreSQL 17/18 through both source and installed checks.
They verify new-name lookup, preserved view outputs, repeated renames, and reuse
of the old name. Rust schema-chain tests retain applicability, binding identity,
source history, failure atomicity, inherited catalog masking, row-type and
qualified FK consumers, sequence/index links, and SQLite identity/oracle checks.
Unqualified FK binding loss and explicit SQLite legacy-PRAGMA propagation are
tracked independently in #810 and #811; this suite does not claim those existing
limitations are repaired.

Issue #613 adds project role inventory fixtures on PostgreSQL 17/18. The optional
`<fixture>.metadata.toml` file appends datasource fields to that fixture's explicitly
selected regression metadata. `project-roles` supplies exact role names;
`project-roles-empty` explicitly supplies an empty list, while
`project-roles-omitted` has no role configuration. All three run through the
shared source and installed checks. Rust tests retain owner bindings, catalog
application, current-configuration isolation, and cache identity assertions.

Issue #484 adds the `collate` project-schema fixture on PostgreSQL 17/18 through
both source and installed checks. It checks explicit name lookup diagnostics,
operand types/nullability, output names, nesting, parameter consumers and GROUPING
matching by resolved collation identity across qualification differences. Rust
tests retain completed binding identities, missing/ambiguous outcomes, nested
operands, serialization and traversal. SQLite feature tests verify its explicit
lookup and mandatory static not-found diagnostics; registration is not implemented.
Expression-wide collation derivation and conflict checking are outside this scope.

Issue #503 adds ordinary `BETWEEN`, `NOT BETWEEN` and explicit `ASYMMETRIC`
contracts in `between.json`, selected with `between.sql` on PostgreSQL 17/18.
The shared source and installed paths cover comparison compatibility, NULL
positions, columns, parameters, conflicts, recovery and exact result metadata.
Untyped NULL/string literals use independent comparison targets; invalid
JSON/NULL comparisons must still report incompatibility.
Nullability retains the shared comparisons' conservative policy; these are
static analysis checks, not predicate evaluation. Rust retains comparison
bindings, serialized public builders, independent operand compatibility and
correlated parameter domains. SQLite semantics remain tracked in #855.

Membership fixtures for #500 cover list and subquery IN/NOT IN, rows, implicit
coercions, NULL inputs, correlated references, parameter-dependent outputs, and
shape/type diagnostics. `membership.json` runs against PostgreSQL 17/18 through
both source and installed checks. Focused Rust tests retain operator/column
bindings, operand retention, SQLite semantics and correlated parameter domains.

`membership-comparisons.json` checks heterogeneous IN/NOT IN lists at the builtin
schema level. An unknown NULL is coerced independently for each comparison when
no scalar-array common type exists; incompatible comparisons still report errors.
These cases run in both source and installed checks.

Issue #799 adds `domain-operators` and `domain-operators-default` on PostgreSQL
17/18 through both maintained paths. They cover source-domain/base comparisons,
nested domains, implicit coercion to/from domains, preferred builtin versus
domain overloads, domains over arrays,
distinctness, membership,
parameters, unknown literals and invalid comparisons, plus shipped
`information_schema.sql_identifier` comparisons with `name` and `text`.
The shared cast repair restores bundled-schema initialization and all 1,802
cases temporarily ignored by #856 and #861; the temporary inventory and bypass
are removed.
Rust tests retain identity/coercion distinctions, inherited/frozen catalog behavior,
exact/domain-only operator signatures, base-type operator ranking and correlated
constraints. Custom operator DDL is not supported by the schema loader; controlled
operator competition remains at the Rust binding/inference boundary.

## Declaration collations

Issue #822 declaration contracts live in `declaration-collations.json`, selected
with the explicit `declaration-collations.sql` project schema on PostgreSQL 17/18.
These cases run in both maintained execution paths. Their expectations check
record-column COLLATE diagnostics,
scalar/array/domain/enum/composite/table-row types, quoted and qualified names,
lookup precedence, and exact result metadata. The missing domain base-type and
function cases also verify the descriptive diagnostic IDs from #878.
DDL inputs exercise accepted and
rejected table, composite and domain declarations, plus rejected add-column
groups. Valid schema DDL supplies subsequent query metadata. Focused Rust tests
retain applicability,
undefined-capability and ambiguity outcomes, requested types/modifiers, rejected
DDL atomicity, public composite-analysis entry-point coverage and recovery.
PostgreSQL ADD COLUMN application and ALTER COLUMN TYPE remain tracked by #528
and #529 respectively; declaration validation does not implement those operations.

The query-analysis API analyzes each input statement in an independent context,
and `sql-fun test` expects one result per case. Catalog transitions and
whole-statement atomicity therefore remain Rust schema-chain assertions; these
CLI cases do not claim cross-statement mutation coverage.

Column rename contracts for #530 live in `column-renames.json`, selected with
`column-renames.sql` on PostgreSQL 17/18 through both maintained execution paths.
The six cases per version check new names, fixed PostgreSQL view outputs,
repeated renames, old-name reuse, quoted names and explicit aliases. Rust
schema-chain tests retain analyzed applicability, bindings, failure atomicity,
source history, layer isolation, FK identities and SQLite oracle comparisons.

## Parallel CI execution

CI runs one shared-server suite using the committed schedule described above.
Each persistent shell worker claims the next fixture/version group when idle
and opens independent CLI connections. Worker metadata and reports are isolated.
The coordinator owns a fresh server and discovery namespace, checks server
identity throughout execution, disables automatic server relaunch, and reaps
its workers and their children. Failed groups are not retried. A group that
exceeds the 1800-second worker protocol deadline fails the run.

After the full suite, the same server revisits conflicting project schemas and
omitted/empty/populated role selections to check cache isolation. Existing
negative controls run once per selected version. Those reports are separate
from the positive-suite combined JUnit.

Reports retain the expected group and named-case inventory, selected binaries,
source revision, worker XML/logs, claims, completion records, combined XML,
summaries and server logs. Exact group/case identities, worker exits, completion
records, report presence, XML counters and outcomes all contribute to failure.
Missing cases never become fabricated passes or skips. Aggregation preserves
failure/error/skipped details; offline contracts cover skipped preservation
because the current CLI has no skipped-case producer. The standard workspace
check maintains these fast queue, report and scheduling contracts.

Issue #486 row-constructor contracts live in `rows.json`, selected with
`rows.sql` for PostgreSQL 17/18 in both source and installed checks. The cases
cover empty/single/implicit/nested rows, ordered member/star projection, field
nullability, aliases, source stars, named/generic record casts (including field
coercions and nested casts), function/array and COALESCE consumers, and invalid
members/arguments. COALESCE and named
composite members retain their existing conservative nullability policy.
Rust tests retain ordered semantic fields, serialization, parameter-field
relationships and internal-list contracts; SQLite tests use the embedded oracle
for scalar grouping, invalid row projection/scalar operands, and legal row
membership and IS comparisons. Existing UPDATE tests cover
legal SQLite tuple assignments. Bare relation-name whole-row binding remains
[#614](https://github.com/kazuk/sql-fun/issues/614); nested explicit rows use the existing qualified-star expansion.

Issue #885 adds paired `postgres18-syntax-17.json` and
`postgres18-syntax-18.json` fixtures for implemented behavior and engine-version
diagnostics (36 cases on 17, 13 on 18). Missing common-AST conversion or analysis
retains explicit `todo!()` panics; focused Rust tests verify those markers rather
than terminating the shared CLI regression server. Utility implementation is
tracked in #891 and remaining constraint semantics in #887.

Issue #508 adds `rows-from.json` with the explicit `rows-from.sql` project
schema for PostgreSQL 17/18 through both maintained paths. It checks SQL-value
and special-expression sources alongside ordinary function outputs, aliases,
ambiguity, ordinality, NULL padding, composites, parameters and recovery.
Rust retains source bindings, serialized semantics, legacy entry-point and
parameter-domain assertions; see [the ROWS FROM inventory](inventory-rows-from.md).

Issue #514 removes the obsolete CASE cast-candidate algorithm. PostgreSQL CASE
always returns through `CaseExpr::resolve_postgres_or_untyped`, with known
results selected eagerly by the shared ordered common-type resolver and
parameter-dependent results retained in the common-type constraint graph.
The remaining builder serves fully typed SQLite expressions only and performs
no PostgreSQL cast selection. Direct unit tests of the removed algorithm were
legacy scaffolding, not a reachable PostgreSQL analysis path.

The existing `case-common-types` fixture now also covers bidirectional
`varchar`/`bpchar` candidates in both ELSE orders, preferred `float8` selection
against `numeric` in both orders, simple CASE with multiple arms, and explicitly
bound parameter results. PostgreSQL 17.6/18.1 oracle probes agree on these result
types. Existing cases retain incompatible-category diagnostics and
unknown/domain/array/nested coverage. The shared manifest runs these contracts
through source and installed verification; Rust retains coercion, comparison,
parameter-domain and recovery assertions.

Issue #501 adds `patterns.json` for PostgreSQL 17/18 through source and installed
verification. Cases cover positive and negative LIKE/ILIKE/SIMILAR TO, explicit,
omitted, empty and NULL escapes, NULL operands, compatible string and bytea
signatures, rejected numeric operands and unresolved references. Rust tests retain
operator/helper bindings, serialized operands and correlated parameter domains;
SQLite feature-enabled tests check builtin LIKE semantics and operand retention.
SQLite GLOB/MATCH/REGEXP classification remains distinct and unsupported semantic
paths retain intentional TODOs owned by #907, #908 and #909 respectively.

Issue #515 set-operation outputs are covered by `set-operations.json` on
PostgreSQL 17/18 through both maintained paths. UNION/INTERSECT/EXCEPT and ALL
cases check common types, left-side names, nullability, shape/type diagnostics,
nested/derived/CTE consumers, domains, arrays, explicit parameter inputs and
INSERT/UPDATE assignments from parameter-free set results.
Rust retains coercion, serialization, parameter-domain, catalog-application and
SQLite contracts; see [the set-operation inventory](inventory-set-operations.md).

Issue #516 extends `rows.json` with GREATEST over anonymous rows, nested
anonymous-record arrays, and COALESCE over structural arrays on PostgreSQL
17/18. Both source and installed paths retain the existing nominal-only output
payload while checking successful analysis and nullability. Rust tests inspect
the owned definitions, field types, array elements, optional names, inference
substitution, compatibility, canonicalization and trigger dependency identities.
Range construction and subtype metadata remain tracked separately in #914.

Issue #520 adds `output-function-params.json` with an explicitly selected
`output-function-params.sql` schema for PostgreSQL 17/18 in both source and
installed checks. It verifies TABLE and INOUT/OUT output order, quoted names,
scalar member types, nullable outputs and a single scalar INOUT result. Rust
unit tests retain incomplete-parameter contracts, owned field types and the
ordered composite return definition.

Issue #518 adds `composite-domains.json` with the explicitly selected
`composite-domains.sql` project schema for PostgreSQL 17/18 through both source
and installed checks. Cases cover domain chains, nested fields, table-row bases,
preserved source/field domain types, parameter consumers, missing fields,
non-composite scalar/array bases and a later independent successful input.
Rust tests retain shared member API outcomes, dynamic definitions, cycle/missing
base safety, semantic operands and correlated parameter assignments.

Issue #858 adds `table-foreign-keys.json` with an explicitly selected parent
schema for PostgreSQL 17/18 through both source and installed checks. Cases
cover named and unnamed CREATE-table foreign keys, omitted referenced columns,
self-reference, attributes, missing source/target columns or relations, and
omitted referenced columns when the parent has no primary key.
Rust schema-chain tests inspect the returned semantic table and catalog,
creation-time bindings, column order, attributes, rename propagation and frozen
schema layers. SQLite feature tests use the embedded oracle for CREATE-table
retention and permitted forward references; SQLite has no ALTER ADD CONSTRAINT
counterpart.
