# table_function_definition_required

## Code and severity

Code: `table_function_definition_required`. Severity: Error.

## When it is reported

A FROM function returns record without declared output columns, but the caller
supplies no column definition list.

## What analysis knows and does not know

The invalid output-declaration condition is retained. Other function lookup,
argument-type or output-column diagnostics can accompany it; this diagnostic
alone does not establish a complete output shape.

## How to address it

Declare the intended output column names and types at the function invocation,
using the actual record shape. Ordinary positional aliases do not specify types.

## Example

PostgreSQL builtin catalog containing json_to_record(json).

```sql
SELECT * FROM json_to_record('{}'::json) AS r;
```

For a record with one text column, a definition would be AS r(value text). The
declared shape must match the intended record contract.

## Related diagnostics

[table_function_definition_not_allowed](table_function_definition_not_allowed.md)
is the opposite case: the output shape is already declared.
