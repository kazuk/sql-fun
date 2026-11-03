# table_function_too_many_aliases

## Code and severity

Code: `table_function_too_many_aliases`. Severity: Error.

## When it is reported

A FROM function or ROWS FROM alias list contains more positional names than
available output columns.

## What analysis knows and does not know

The alias-count failure is retained. Output shape may already be known; an alias
list only renames columns and does not create extra outputs.

## How to address it

Reduce the alias list to the actual output width, or change the function
selection if additional outputs are intended. Count an explicitly requested
ordinality column when applicable.

## Example

PostgreSQL builtin catalog containing generate_series(integer, integer).

```sql
SELECT * FROM generate_series(1, 2) AS r(value, extra);
```

The function returns one column; extra cannot name a second output.

## Related diagnostics

[table_function_definition_required](table_function_definition_required.md)
concerns typed record declarations;
[table_function_duplicate_column](table_function_duplicate_column.md) concerns
repeated declared names.
