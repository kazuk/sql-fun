# table_function_definition_not_allowed

## Code and severity

Code: `table_function_definition_not_allowed`. Severity: Error.

## When it is reported

A caller supplies typed column definitions for a FROM function that does not
require an anonymous record definition, because its result shape is already
declared.

## What analysis knows and does not know

The invalid output-declaration condition is retained. Other function lookup,
argument-type or output-column diagnostics can accompany it; this diagnostic
alone does not establish a complete output shape.

## How to address it

Remove the typed definition list. Use plain positional column aliases if
renaming existing outputs is intended.

## Example

PostgreSQL builtin catalog containing generate_series(integer, integer).

```sql
SELECT * FROM generate_series(1, 2) AS r(value integer);
```

The integer result is already defined; AS r(value) renames it without
redeclaring its type.

## Related diagnostics

[table_function_definition_required](table_function_definition_required.md)
concerns a record result whose shape is not declared.
