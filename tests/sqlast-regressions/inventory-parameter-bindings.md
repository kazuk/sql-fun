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
  analyze selected-query.sql --schema-level project
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
