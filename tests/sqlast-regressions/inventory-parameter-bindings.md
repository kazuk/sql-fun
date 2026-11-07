# Application input binding evidence (#766)

The maintainer confirmed that `parameter_type_unresolved` remains an error when
analysis cannot finalize an application's input binding. PostgreSQL's untyped
PREPARE signature is separate from the retained admissible input domains.

## PostgreSQL oracle

PostgreSQL 17.6 and 18.1 both accepted these statements in a transaction:

```sql
CREATE FUNCTION public.issue766_record_fun(src text) RETURNS record
  LANGUAGE sql IMMUTABLE AS 'SELECT 1, ''x''';
PREPARE issue766_record AS
  SELECT a,b FROM public.issue766_record_fun($1::text) AS r(a int,b text);
PREPARE issue766_plain AS SELECT $1::text;
PREPARE issue766_array AS SELECT (ARRAY[1,2])[$1] AS n;
SELECT name, parameter_types FROM pg_prepared_statements
  WHERE name LIKE 'issue766_%' ORDER BY name;
```

The reported types were `{integer}`, `{text}`, `{text}` for `issue766_array`,
`issue766_plain`, `issue766_record`, respectively. All statements were deallocated
and the transaction rolled back. No prepared query was executed.

## CLI evidence and maintained coverage

The baseline CLI/server was built from `4bcd728a`. Both actual bundled catalog
installations (`sql-fun-home/postgres/17` and `18`) produced the same results.
Each probe explicitly selected a metadata datasource with that version,
`search_path = ["public"]`, and the existing `schema-table-functions.sql` dump:

```sh
sql-fun --term-color never --metadata-file metadata-17.toml \
  --datasource regression \
  --schema-file tests/sqlast-regressions/schema-table-functions.sql \
  analyze --sql-file selected-query.sql --schema-level project
```

Repeat with metadata selecting version 18. The `parameter-bindings` fixture
retains the exact SQL and expected complete diagnostic-code sets, ordered column
types and nullability from these probes. The shared runner checks these cases
in both source-tree and installed-distribution verification.

| Cases | Analysis exit | Complete | Public parameter type | Diagnostics |
| --- | --- | --- | --- | --- |
| Native/named plain result cast | 1 | false | absent | `parameter_type_unresolved` |
| Native/named record call with result cast | 1 | false | absent | `parameter_type_unresolved` |
| Explicit `text` input hint, plain/record/repeated use | 0 | true | `text` | none |
| Unrestricted `IS NULL` | 1 | false | absent | `parameter_type_unresolved` |
| Conflicting HAVING/LIMIT requirements | 1 | false | absent | `parameter_constraint` |
| Unhinted array index | 1 | false | absent | `parameter_type_unresolved` |
| Explicit `int4` array index | 0 | true | `int4` | none |
| Incompatible `bool` array index | 1 | false | absent | `array_index_type_mismatch`, `parameter_constraint` |
| Independent literal query | 0 | true | no parameter | none |

For the motivating native query, the public result body was:

```text
  Query kind: Select
  Complete: false
  Executable SQL:
    SELECT "a", "b" FROM "record_fun"(CAST($1 AS "text")) AS "r"("a" int, "b" "text")
  Parameters (1):
    $1 $1: unresolved; unknown
  Result columns (2):
    [0] a: pg_catalog.int4; unknown
    [1] b: pg_catalog.text; unknown
  Column access (0):
```

The baseline diagnostic code was `parameter_type_unresolved`, with message
`SQL type of parameter $1 is unresolved; supply an explicit SQL type hint`.
The revised message explains the application binding and the distinction from
result casts; both rendering paths have one message implementation and an exact
message/severity regression. Neither error severity nor the solver changed.

Replacing `$1` with `${src:text}` produced `Complete: true`, a public
`$1 src: pg_catalog.text; unknown` binding and exit 0, with the same known result
columns and no diagnostics. `${src}::text` remained incomplete. A repeated
`${src}` after `${src:text}` retained one binding. The incompatible array hint
produced a conflict rather than being accepted or selecting another type.

The internal finalization test distinguishes empty, singleton, non-singleton
and unrestricted domains without changing those domains. RPC coverage checks
completion and error severity independently of known result columns. The Scrut
CLI suite checks exit 1 with an unresolved application input, a later successful
hinted input, and exit 0 when `test` expects the error and known columns. A
passing contract test does not assert a finalized binding for its input SQL.

## First-occurrence application bindings (#779)

The #766 observations above are historical evidence of the redundant-hint DX
problem. Before implementation, CLI/server binaries built from `71acf967`
reproduced the unhinted plain-cast and record-function failures on both bundled
PostgreSQL 17/18 catalogs. `${src:text}` completed successfully with a `text`
parameter and the same known result columns. The additional inputs
`SELECT ${src}::text, ${src} + 1` and
`SELECT ${src} IS NULL, ${src}::text` both reported
`parameter_type_unresolved` on that baseline.

The current `analyze` command requires explicit file selection:

```sh
sql-fun --term-color never --metadata-file metadata-17.toml \
  --datasource regression \
  --schema-file tests/sqlast-regressions/schema-table-functions.sql \
  analyze --sql-file selected-query.sql --schema-level project
```

The updated fixture retains the plain/record cases as successful `text`
bindings without extra hints. It adds ambiguous-first, later operator rejection,
a hint written on a later occurrence, and source-order versus semantic-traversal
coverage. Native and named inputs share the numbered SQLAST parameter contract.
The existing manifest runs these contracts through source and installed checks
for both catalog versions.

Existing cast, array, truth-test, comparison and DML fixtures also reflect the
new policy. Scalar identity cases no longer expect unresolved inputs, and
result columns use selected scalar inputs. A first unconstrained use remains
ambiguous despite a later assignment or comparison. SQL-established arrays,
records, UUIDs and other types outside the seven candidates remain available
to SQL-admissibility inspection, but require an explicit input hint when no
default candidate is accepted. Their fixtures retain known result columns and
now assert the unresolved application binding separately.

Focused Rust coverage checks first-occurrence order, authoritative resolved hints,
retention of the selected type after rejection, and continued SQL-admissible
assignment inspection. RPC coverage checks public parameter type and completion,
including an ambiguous first occurrence in a CTE. Ambiguity payloads retain the
local candidates; conflict payloads retain the parameter, selected type, and
conflicting requirements. The CLI small-catalog contract also checks a
successful unhinted identity cast,
selected parameter metadata, mixed-input recovery, and the cast-only exit status.
Parser source locations are analysis bookkeeping and
are not added to the serialized semantic parameter contract.

The PostgreSQL 17 and 18 oracles reject `PREPARE issue779_bad(text) AS
SELECT $1::text, $1 + 1` with `operator does not exist: text + integer`.
The prepared statement is not created, and no query is executed. The first-use
binding policy itself is sql-fun-defined, not inferred from PREPARE behavior.

An indirect predicate case retains failure of the complete application
constraints: `binding_predicate(int4)` returns `text`, while its `int8` overload
returns `bool`. After a first `$1::int4`, using `binding_predicate($1)` in WHERE
must fail, even though an explicitly supplied `int8` input could satisfy the
SQL. Both PostgreSQL 17 and 18 reject the corresponding `PREPARE ... (int4)`
with `argument of WHERE must be type boolean, not type text`; the small function
probes ran inside rolled-back transactions.

PR #1033's review added an internal regression for resumed collection after
composite-star expansion: `SELECT $2::text, ARRAY(SELECT ($1).*),
take_singleton($1)`. The selected application binding of `$2` stays `text`,
while SQL-admissibility inspection must still accept an explicit `int4` input.
The small in-memory catalog test checks both contracts and the expanded
single-column projection. PostgreSQL 17 and 18 accepted the corresponding
typed PREPARE with a singleton composite and an integer input; both probes
deallocated the statement and rolled back without executing the prepared query.
