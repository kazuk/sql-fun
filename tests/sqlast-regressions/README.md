# SQLAST semantic regression contracts

This internal suite migrates SQL-observable assertions from builtin-dependent
SQLAST Rust tests to the real CLI → client → server analysis path. The manifest
is shared by source verification (`check-test-flow` workspace step) and installed
verification (`postgres-extraction`, after initialization and catalog comparison).
No regression SQL is executed against PostgreSQL.

`manifest.tsv` explicitly selects each fixture, schema level, optional project
schema, expected case count and PostgreSQL versions. Each JSON fixture is an
array of self-contained cases with `name`, `sql` and `expected` fields. Keep each
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

The pilot asserts success and an expected diagnostic. Negative controls introduce
wrong result-column and diagnostic expectations, verify exit status 1 and readable
expected/actual differences, and confirm a later case still passes. An invalid
selector and an unconfigured datasource must also fail. Expected-failure evidence
lives under `controls/` so it can be inspected without treating it as a failing
regression JUnit suite.

The inventory documents migrated cases and internal assertions retained in Rust.
AST structure, binding identity, parameter domains, internal error contracts and
schema-layer state transitions stay at their existing unit/integration boundary.

The accepted scope for issue #757 is builtin-only migration. Tests requiring
project/default SQL schemas remain in Rust while [#727](https://github.com/kazuk/sql-fun/issues/727)
tracks shipped-schema diagnostics. No catalog substitution or diagnostic
suppression is used. The current manifest executes 228 positive contract inputs
(112 for PostgreSQL 17 and 116 for PostgreSQL 18), plus three negative-control
inputs, an invalid-selector check and a missing-datasource check for each version.

See [the broad inventory](inventory-retained.md) and the detailed
[ARRAY/min-max](inventory-arrays.md), [CASE/COALESCE](inventory-expressions.md),
[query/type/view](inventory-query.md), [cast](inventory-casts.md), and
[vector](inventory-vectors.md) mappings for preserved scenarios and reasons that
internal assertions remain in Rust. Counts refer to SQL inputs, not removed Rust
tests: one old test may check multiple SQL expressions, and mixed tests retain
internal assertions after their observable result assertions move to the CLI.

The runner prints each manifest group's result, executed/expected case counts
and wall-clock seconds measured with Bash `time` around the actual `sql-fun test`
command. The final summary aggregates those command durations and reports total
executed cases and cases/second. Build, server startup, catalog preparation,
fixture projection, report processing/upload and negative controls are excluded
from this timing. These values provide a baseline in source and installed CI
logs; timing variation does not affect success and no performance threshold is
enforced.
