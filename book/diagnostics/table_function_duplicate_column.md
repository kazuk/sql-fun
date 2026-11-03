# table_function_duplicate_column

## Code and severity

Code: `table_function_duplicate_column`. Severity: Error.

## When it is reported

A caller-supplied record definition repeats an output column name.

## What analysis knows and does not know

The repeated name is retained. This is a duplicate within one typed definition,
not an ambiguity between separate FROM sources.

## How to address it

Give each declared output column a distinct name matching the intended record
shape.

## Example

PostgreSQL builtin catalog containing json_to_record(json).

```sql
SELECT * FROM json_to_record('{}'::json) AS r(value integer, value text);
```

Both declarations use value; differing types do not make the names distinct.

## Related diagnostics

[ambiguous_column](ambiguous_column.md) concerns reference lookup with multiple
matches. [table_function_too_many_aliases](table_function_too_many_aliases.md)
concerns alias count.
