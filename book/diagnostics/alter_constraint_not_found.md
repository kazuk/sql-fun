# alter_constraint_not_found

## Code and severity

Code: `alter_constraint_not_found`. Severity: Error.

## When it is reported

The target table has no retained constraint matching the requested name.

## What analysis knows and does not know

The analyzer uses the selected PostgreSQL catalog and the retained declarations.

## How to address it

Check the declared name. For DROP, use IF EXISTS when absence is acceptable.

## Example

```sql
ALTER TABLE items DROP CONSTRAINT missing;
```

## Related diagnostics

[relation_not_found](relation_not_found.md)
