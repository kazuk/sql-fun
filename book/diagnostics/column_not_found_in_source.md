# column_not_found_in_source

## Code and severity

Code: `column_not_found_in_source`. Severity: Error.

## When it is reported

A column reference cannot be found in the visible query sources. This includes
an unqualified name in a query without a FROM source.

## What analysis knows and does not know

The referenced name and source description are retained. The analyzer has not
established a binding or a type for that column. This does not establish that
your live database lacks the column: analysis uses the selected catalog and
schema.

## How to address it

Check the spelling, visible aliases and query scope. If the column belongs to a
table, include that table in FROM and ensure the selected schema contains it.

## Example

PostgreSQL 17–18 builtin schema; no project tables are needed.

```sql
SELECT missing AS result;
```

`missing` has no visible source. Replace it with a defined expression, for
example `SELECT 1 AS result`, or bind the intended table and column.

## Related diagnostics

[column_not_found_in_table](column_not_found_in_table.md) concerns a known table
source. [ambiguous_column](ambiguous_column.md) means more than one matching
column was found.
