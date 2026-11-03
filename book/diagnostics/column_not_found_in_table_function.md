# column_not_found_in_table_function

## Code and severity

Code: `column_not_found_in_table_function`. Severity: Error.

## When it is reported

A resolved table-function source does not expose the requested column.

## What analysis knows and does not know

The source alias and column name are retained. Result-shape metadata is separate
from successful function name and argument resolution.

## How to address it

Use the returned column name or declare a supported column alias list that
matches the actual result shape. For record-returning functions, provide a
required column definition list where supported.

## Example

PostgreSQL 18 builtin catalog, including `generate_series(integer, integer)`.

```sql
SELECT f.missing FROM generate_series(1, 2) AS f;
```

The scalar result is exposed under its source name; `f.missing` does not name
that output.

## Related diagnostics

[table_function_not_found](table_function_not_found.md) reports the function
source itself did not resolve.
[table_function_definition_required](table_function_definition_required.md)
reports a record result needs declared columns.
