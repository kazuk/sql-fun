# SQLAST semantic regression contracts

This internal suite migrates SQL-observable assertions from builtin-dependent
and schema-dependent SQLAST Rust tests to the real CLI → client → server
analysis
path. The manifest is shared by source verification (`check-test-flow` workspace
step) and installed verification (`postgres-extraction`, after initialization
and catalog comparison).
No regression SQL is executed against PostgreSQL.

`manifest.tsv` explicitly selects each fixture, schema level, optional project
schema, expected case count and PostgreSQL versions. Each JSON fixture is an
array of self-contained cases with `name`, `sql` and `expected` fields. Keep
each
SQL input and its expectation in the same object when adding, deleting or
reordering cases. Names preserve original Rust case identities and must be
nonempty and unique within the fixture. The runner validates object fields and
case counts, then projects cases to temporary SQL and expectation arrays for the
existing CLI input contract. CLI/JUnit identities retain the fixture basename
and array index; each group log also maps those indices to case names.
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
manifest totals are 694 inputs for PostgreSQL 17 and 700 for
PostgreSQL 18 (1394 total). These count positive contract inputs separately from
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
fixture projection, report processing/upload and negative controls are excluded
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
