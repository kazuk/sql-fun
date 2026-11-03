# table_function_invalid_typmod

## Code and severity

Code: `table_function_invalid_typmod`. Severity: Error.

## When it is reported

A caller-declared record column uses type modifiers that fail the supported
PostgreSQL modifier validation.

## What analysis knows and does not know

The validation explanation is retained. The base type can be resolved even
though its declaration-local modifiers are invalid.

## How to address it

Correct the modifier count and range for that type. Do not treat every
unrecognized modifier implementation as proof of invalid SQL.

## Example

PostgreSQL builtin catalog containing json_to_record(json) and varchar.

```sql
SELECT * FROM json_to_record('{}'::json) AS r(value varchar(0));
```

The varchar length must be positive. This is distinct from a missing type name.

## Related diagnostics

[table_function_type_not_found](table_function_type_not_found.md) means the base
type could not be resolved.
