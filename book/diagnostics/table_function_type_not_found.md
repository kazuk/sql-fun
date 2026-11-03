# table_function_type_not_found

## Code and severity

Code: `table_function_type_not_found`. Severity: Error.

## When it is reported

A type named in a caller-supplied record column definition cannot be resolved in
the selected catalog.

## What analysis knows and does not know

The requested type name is retained. The column name remains known, but its
resolved type is unavailable. A later collation check cannot establish type
applicability until this is resolved.

## How to address it

Correct or qualify the type name and load any required type declaration.

## Example

PostgreSQL builtin catalog containing json_to_record(json); no
public.absent_type is declared.

```sql
SELECT * FROM json_to_record('{}'::json) AS r(value public.absent_type);
```

This concerns the caller-declared output type, not the function argument or
function name.

## Related diagnostics

[column_type_not_found](column_type_not_found.md) concerns another column-type
lookup context. [collation_not_found](collation_not_found.md) concerns an
unresolved explicit collation.
