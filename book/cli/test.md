# Test SQL contracts

`sql-fun test` checks diagnostic-code sets and statically analyzed result columns.
See the [diagnostic reference](../diagnostics/README.md) for code meanings and
suggested corrections.

It supports both SQLAST regressions and application query-interface tests without
executing queries or connecting to a live database. It uses the same
datasource selection, schema levels, and server prerequisites as
[`sql-fun analyze`](analyze.md), and never creates or validates Rust query lockfiles.
Its legacy SQL-source form also uses the same input syntax as `analyze`.

`test` does not request the optional relation/column access analysis that
[`analyze`](analyze.md#relation-and-column-access) enables by default.

## Syntax and options

```text
sql-fun [COMMON OPTIONS] test [OPTIONS] <INPUT>
```

`INPUT` is a filename, optionally followed by `#JSONPath`.
[Common options](index.md#common-options) precede `test`.

| Option | Behavior |
| --- | --- |
| `--schema-level <LEVEL>` | `builtin`, `default`, `extensions`, or `project`; default `project`. |
| `--expect <FILE[#JSONPath]>` | Use a separate expectation source with legacy SQL input. |
| `--ignore-schema-diagnostics <LEVEL>` | Continue through `warning` or `error` schema diagnostics, as described below. |
| `--junit-xml <PATH>` | Write a JUnit report to this path. |
| `-h`, `--help` | Print help. |

These command-specific options have no environment variable equivalents.

## Named case arrays

Pass a JSON, TOML, or YAML case array without `--expect` to keep each SQL input
and its expected contract together:

```sh
sql-fun --metadata-file Cargo.toml --datasource app \
  test cases.json --junit-xml target/sql-fun-test/junit.xml
```

For example, `cases.json` can assert a literal's result columns:

```json
[
  {
    "name": "select-literal",
    "sql": "SELECT 1 AS value",
    "expected": {
      "diagnostics": [],
      "columns": [
        {
          "name": "value",
          "type": {
            "schema_name": "pg_catalog",
            "local_name": "int4",
            "is_array": false
          },
          "nullability": "not_null"
        }
      ]
    }
  }
]
```

The document root must be a nonempty case array. An optional `#JSONPath` can
select an array inside a structured document, for example
`'cases.toml#$.tests'`; it must match exactly one node. Each case requires a
nonempty string `name`, a string `sql`, and an `expected` contract using the
[expectation rules below](#expectations), including diagnostic-array shorthand.
Names must be unique within the selected array, using exact, case-sensitive
comparison. Names are preserved without trimming. Unknown fields and malformed
cases are input errors. All cases are validated before analysis or server
connection begins. SQL text retains its existing semantics: zero or multiple
statement results fail the assertion.

Without `--expect`, only the case-array form is accepted. With `--expect`, the
input is a legacy SQL source and the separate expectation source is required;
case objects are not accepted as SQL strings.

## SQL sources with separate expectations

The existing SQL-source interface remains available:

```sh
sql-fun --metadata-file Cargo.toml --datasource app \
  test 'cases.toml#$.tests.queries' \
  --expect 'cases.toml#$.tests.expected' --junit-xml target/sql-fun-test/junit.xml
```

For example, with a schema defining `events(id int4 NOT NULL)`:

```toml
[tests]
queries = [
  "SELECT id FROM events",
  "INSERT INTO events (id) VALUES (1)",
]

[[tests.expected]]
diagnostics = []
[[tests.expected.columns]]
name = "id"
type = { schema_name = "pg_catalog", local_name = "int4", is_array = false }
nullability = "not_null"

[[tests.expected]]
diagnostics = []
# Omitted columns asserts zero result columns.
```

## Unresolved input bindings

`parameter_type_unresolved` is an error from analysis when the application's
input binding cannot be finalized. It does not declare the SQL intrinsically
invalid. Known output columns are still checked even when an input type is
unresolved. For example, this contract expects the error and the known `text`
result from a result cast:

```toml
[tests]
queries = ["SELECT $1::text AS value"]

[[tests.expected]]
diagnostics = ["parameter_type_unresolved"]
[[tests.expected.columns]]
name = "value"
type = { schema_name = "pg_catalog", local_name = "text", is_array = false }
nullability = "unknown"
```

This test passes with status 0 when the diagnostic and column contract match;
it does not assert a finalized executable application binding. An unexpected
diagnostic or result-column difference still fails. The same input makes
`analyze` exit with status 1. To explicitly select the input type, use
`SELECT ${src:text}::text AS value`, whose expected diagnostic set is empty.
See [parameter bindings](analyze.md#parameter-bindings).

## Expectations

`--expect` reads a JSON, TOML, or YAML file, optionally followed by `#JSONPath`.
Without a selector, its structured document root is the expectation value.
A selector must match exactly one node. One SQL string takes one expectation
object; a SQL array takes an array of expectations with the same length, matched
by zero-based index. A one-element SQL array still requires an expectation array.
All input structures and counts are validated before analysis begins.

Every expectation object requires `diagnostics`, an array of actual API diagnostic
codes. Codes are compared as sets: order and duplicates are ignored, but both
missing and unexpected codes fail. All severities participate. An empty set
requires no diagnostics of any severity. SQL semantic errors can be expected;
API, transport, and internal analysis failures cannot be expected as codes.

`columns` is always checked. Omitting it means zero columns, exactly like
`columns = []`; omission never skips validation. Nonempty columns are compared
in ordinal order, checking the name, optional SQL type, and nullability.

- A present `type` requires `schema_name`, `local_name`, and explicit `is_array`.
  Use exact API names, including case, rather than SQL aliases.
- Omitting `type` (or setting it to null in JSON/YAML) asserts that the API type
  is absent/unresolved. It is not a wildcard.
- Each column requires `nullability`: `not_null`, `nullable`, or `unknown`.
- Unknown fields and malformed expectations are input errors.

A diagnostic-code array is also accepted as shorthand for an expectation with
those diagnostics and zero columns. It still checks result shape.

One selected SQL string must produce exactly one statement analysis result.
Zero or multiple statement results fail the assertion. The command never combines
statement diagnostic sets or result columns to match one expectation.

The result reflects static analysis, including its unresolved types and
nullability. It does not verify database contents or execute query effects.

## Schema diagnostics

Schema initialization diagnostics are always displayed, independently of the
query case results. By default, error-level schema diagnostics stop the tests;
warnings do not. To continue despite recoverable schema errors, pass
`--ignore-schema-diagnostics error` after `test`. The loaded schema may be
incomplete, so query results may reflect missing definitions. The value `warning`
also permits warnings but does not permit errors, matching the default behavior.
This option changes whether diagnostics stop testing; it does not hide them or
bypass schema loading failures, missing contexts, or transport errors.

## Reports and exit status

Console output identifies named cases as `<source-or-selector>::<case-name>`.
Legacy SQL sources retain their source/selector and array-index identities.
Passing cases print only `PASS` and their identity; expected query diagnostics are
not displayed. Failed cases print `FAIL` before their expected/actual differences
and relevant actual diagnostic context, including case and statement indices.
Execution errors are reported as `ERROR`. A final case count is printed.
`--term-color auto` enables colors for a terminal; `never` disables them and
`ansi` explicitly enables them. Redirected automatic output is plain text.
Independent cases continue after assertion failures and attributable API errors.

`--junit-xml <PATH>` writes one testcase per selected SQL input, creates parent
directories, and produces a complete report even when cases fail. Matching cases
pass; diagnostic, column, and statement-result-count mismatches are `failure`
elements. Attributable setup/API/transport/internal failures are `error` elements.
Invalid input before cases can be enumerated is a command-level error and does not
create invented testcases. Named JUnit testcase identities use the supplied
source/selector and case name; legacy testcase identities retain the supplied
source, selector, and array index. XML 1.0-disallowed characters in report text
are replaced with the Unicode replacement character. The report supports JUnit
consumers such as the repository's existing GitHub Actions test reporter. No
report file is accessed when the option is absent.

Exit status is 0 only when every case passes, 1 for assertion or execution errors
(including report-write failure), and 2 for invalid CLI usage. Producing a JUnit
report does not turn a failing run into success.
