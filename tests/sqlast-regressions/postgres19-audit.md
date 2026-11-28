# PostgreSQL 19 fixture audit for issue #1135

The [case ledger](postgres19-audit.tsv) records a one-time comparison of the
1,877 distinct fixture inputs at commit `b9461e9e4d550b8f38acb2ab3d20e76fa06fb1db`.
Look up each case's expected diagnostics and ordered output fields in the JSON
file named by its fixture. The ledger includes the 39 inputs used exclusively
to test the PostgreSQL 17 version gate. Their 17 expectations stay in place.

The builder provided PostgreSQL 17.6, 18.1 and 19.0. The audit used the real
server catalogs, the manifest-selected schema and datasource roles, and
explicit public search path. Schema setup and utility statements ran in
transactions with per-case savepoints and rollback. For queries and
data-modifying statements, the extended query protocol supplied parse/describe
results without executing their data operations. Named application inputs were
mapped to native parameter positions with explicit type OIDs where supplied.
Targeted EXPLAIN probes checked cases where a planner step was material to the
fixture's expectation.

## Observations

| Case disposition | Count |
| --- | ---: |
| PostgreSQL accepts with matching described names and types | 1,015 |
| PostgreSQL rejects with a CLI diagnostic expected | 512 |
| Further EXPLAIN probe establishes rejection | 4 |
| Descriptor differs | 107 |
| CLI policy or rendering diagnostic with PostgreSQL parse acceptance | 64 |
| Oracle setup unavailable or non-equivalent | 132 |
| PostgreSQL 17-only version-gate input | 39 |
| Typed versus untyped native input contract | 1 |
| Analyzer accepts PostgreSQL-rejected SQL | 1 |
| Analyzer rejects PostgreSQL-accepted SQL | 1 |
| PostgreSQL 19 changes the reason for rejection | 1 |

The protocol covers ordered labels and wire type identities. It reports
base types for domain outputs and the generic `record` identity for anonymous
rows. Separate `pg_typeof` probes confirmed that an identical-domain CASE
retains its domain and that anonymous rows and arrays of those rows have SQL
types `record` and `record[]`. The CLI can retain domain
identity or omit a type that it cannot assign a named SQL identity to. An empty
CLI label versus PostgreSQL's generic `?column?` is also a representation
difference. Meaningful labels lost by analysis are tracked in
[#1286](https://github.com/kazuk/sql-fun/issues/1286). An inferred PostgreSQL
type for a native parameter also does not establish that sql-fun can select
one application binding or one result type under its input policy. For example,
an untyped preparation of `UPDATE items SET id=$1,label=$1` reports `42P08`;
preparing it with an explicit integer parameter is valid.

Row descriptions do not expose result nullability. The audit retains those
assertions as analysis contracts. Targeted PostgreSQL 19 probes confirmed
NULL from an array NULL element and out-of-range access, a non-NULL array
from an empty subquery, outer-join null extension using unmatched VALUES
rows, and the results of an omitted CASE ELSE and a COALESCE with a non-NULL
fallback. These witnesses cover those specific inputs. The ledger does not
claim independent field-by-field nullability verification.

PostgreSQL reports the first error in a statement. The `server-rejects`
entries show the actual SQLSTATE and primary message. They establish the
rejection and provide a cross-check for the CLI diagnostic, which can include
additional findings and recovery metadata. The oracle does not produce
sql-fun lint, application binding or rendering diagnostics. Non-equivalent
setup includes fixtures with synthetic extensions, fake C callbacks, missing
access-method handlers or intentionally incompatible binary layout declarations.
Those rows retain their setup evidence and remain covered by the offline CLI
runner. The source and installed regression lanes continue to use the
fixture-defined catalogs and extensions.

## Follow-ups

- Nested anonymous field access passes analysis and is rejected by all three
  oracle versions. [#1287](https://github.com/kazuk/sql-fun/issues/1287)
- Missing pseudotype arrays change from a duplicate-shell rejection on 17/18
  to undefined type on 19. [#1288](https://github.com/kazuk/sql-fun/issues/1288)
- A PostgreSQL 17 version-gate input exposes a change between 18 and 19 when
  a primary-key column already has a NOT NULL constraint.
  [#1289](https://github.com/kazuk/sql-fun/issues/1289)
- PostgreSQL plans a MERGE with `ON 1`; the fixture expects a query analysis
  error. [#1290](https://github.com/kazuk/sql-fun/issues/1290)
- PostgreSQL describes FROM/USING source columns in UPDATE/DELETE RETURNING
  `*`. At the audited commit the fixtures described only the destination
  columns. The corrected fixtures include the source columns.
  [#1291](https://github.com/kazuk/sql-fun/issues/1291)

Existing rendering gaps are recorded in the ledger with their tracking issues.
The 19 selection mirrors the 18 selection and retains the current analyzer
assertions, including the tracked differences. The oracle review is development
evidence and does not run during local or CI regression verification.
