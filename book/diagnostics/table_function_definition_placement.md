# table_function_definition_placement

## Code and severity

Code: `table_function_definition_placement`. Severity: Error.

## When it is reported

An outer typed column-definition list is used with more than one ROWS FROM
function, per-function definitions, or WITH ORDINALITY.

## What analysis knows and does not know

The invalid output-declaration condition is retained. Other function lookup,
argument-type or output-column diagnostics can accompany it; this diagnostic
alone does not establish a complete output shape.

## How to address it

Put the record definition beside the individual record-returning function inside
ROWS FROM. Keep the outer alias list untyped, and do not redefine the ordinality
column.

## Example

PostgreSQL builtin catalog containing json_to_record(json).

```sql
SELECT * FROM ROWS FROM(json_to_record('{}'::json)) WITH ORDINALITY AS r(value text);
```

The outer definition conflicts with WITH ORDINALITY. Use a per-function
definition before adding ordinality.

## Related diagnostics

[table_function_definition_required](table_function_definition_required.md)
concerns an absent definition rather than its placement.
