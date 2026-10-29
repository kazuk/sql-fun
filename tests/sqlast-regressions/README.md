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
in the committed file. The current 140 groups are split 10/65/65.

CI's archived execution phases validate that the committed tiers exactly cover
the manifest, without duplicates or unknown groups, and have canonical ordering
and sizes. They then run one shared-server suite with **two workers**, consuming
heavy, medium and light in order. There is no calibration, duration estimation,
or priority regeneration in CI. Missing or stale classification is an error.
Default and all-features jobs retain independent binary producers and gates.

CI evidence is retained under fresh `run-*` subdirectories in
`target/sqlast-regressions/`, including the selected priorities, environment,
original inventory, worker reports, combined XML, claims, summaries and resource
samples. Repeating an execution phase preserves earlier evidence. The direct
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
manifest totals are 1103 inputs for PostgreSQL 17 and 1109 for
PostgreSQL 18 (2212 total). These count positive contract inputs separately from
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
lookup precedence, and exact result metadata. DDL inputs exercise accepted and
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

## Opt-in shared-server parallel experiment (#870)

Maintainers can reproduce the experiment after explicitly selecting a real
builtin installation and prebuilt CLI/server from the same revision. It needs
Bash, jq, Python 3 (standard library only), and Linux `/proc`. It does not execute
SQL against PostgreSQL and does not use a developer's existing analysis server.
From the repository root, with the selected binaries already built:

```sh
SQL_FUN_HOME="$PWD/sql-fun-home" \
SQL_FUN_BIN="$PWD/target/debug/sql-fun" \
SQL_FUN_TEST_SERVER_BIN="$PWD/target/debug/sql-fun-server" \
python3 -B tests/sqlast-regressions/experiment.py \
  --workers 1 2 4 --repetitions 3 --output /tmp/sql-fun-parallel-results
```

The output directory must not exist. Worker choices and repetitions are required;
start the distinct worker-count sequence with `1`. A separate serial calibration
trial supplies
environment-specific group duration estimates. All compared trials use a single
queue ordered by descending estimated duration (manifest order breaks ties).
Each persistent shell worker atomically receives its next indivisible
fixture/version group from the coordinator when idle. Failed tasks are not
retried or hidden. Trials rotate configuration order after the serial
calibration and reuse the selected binaries/catalogs. Keep the machine otherwise
idle during comparisons; local speedups cannot select the CI default.

Every trial owns a fresh server and test-only discovery namespace. Each CLI call
uses an independent connection, while worker metadata and reports remain isolated.
The parent checks server identity throughout execution, disables client automatic
server relaunch, reaps workers and their children, and stops only its owned server.
A group exceeding the 1800-second worker protocol deadline fails the experiment.
After the positive suite, the same server revisits conflicting project schemas
and omitted/empty/populated role selections to check cache isolation. Existing
negative controls also run once per selected version. Their reports are separate
from the positive-suite combined JUnit, as are the isolation revisit reports.

`inventory.json` records expected groups and named case identities.
`estimates.json` records the serial calibration duration measurements. Each trial
keeps
raw worker XML/logs, completion records, `combined.xml`, `summary.json`, server
logs, and `/proc` CPU/RSS/peak-RSS/thread samples. Exact case/group reconciliation,
worker exits, completion records, report presence, XML counters, and test outcomes
all contribute to overall failure. Missing cases never become fabricated passes
or skips. Aggregation preserves failure/error/skipped details; the current CLI
has no skipped-case producer, so synthetic contracts verify skipped preservation.
The standard workspace check runs the fast queue/report contracts, while normal
direct `run.sh` source/installed invocation remains serial. CI uses the
committed tiers described above.

`calibration.json` records the separate estimate-establishment run.
`results.json` retains every completed comparison trial, including failed trials.
A failure
stops further comparisons. Successful experiments also write `comparison.json`
with individual end-to-end wall times, medians, ranges, and speedups over the
serial median. Per-trial summaries distinguish positive-worker-phase elapsed
time, worker durations, summed CLI command time, throughput, and full wall time
(including startup, isolation revisits, controls, reporting, and cleanup). Summed
command time is not parallel wall time. Each primary trial starts with an empty
server process cache; the operating system page cache is not cleared. Resource
samples are observations, not proof of a particular lock bottleneck.

After local 1/2/4 correctness is established, explicitly dispatch `build-test.yml`
on the experiment branch with the `regression-benchmark` boolean input enabled.
This optional job uses the normal default-feature CLI/server artifacts from that
same workflow run and the same runner/container selection as workspace execution.
It benchmarks 1/2/4/8 workers three times, uploading the
`regression-benchmark` artifact even on failure (30-day retention). Other normal
workflow jobs still run; the benchmark never rebuilds each trial's binaries.
Record the run URL, revision, runner/image, CPU/memory, cache conditions, raw
results, stability and extra CI cost in the findings. Choose a recommendation
from actual CI measurements, including oversubscription, rather than assuming a
worker count equal to vCPU count.

The calibration-based benchmark stays opt-in; normal CI uses the approved
two-worker committed-tier schedule described above. File separate bug issues
with reproduction evidence for defects
discovered to arise from concurrent server processing; link them to #870 rather
than silently repairing or bypassing them in this experiment.

See the [findings](parallel-findings.md) for repeated local measurements and the
CI recommendation/evidence location.
