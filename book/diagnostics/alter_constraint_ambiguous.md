# alter_constraint_ambiguous

## Code and severity

Code: `alter_constraint_ambiguous`. Severity: Error.

## When it is reported

More than one retained constraint declaration matches the requested name.

## What analysis knows and does not know

The analyzer uses the selected PostgreSQL catalog and the retained declarations.

## How to address it

Give the declarations distinct names in the schema.

## Example

```sql
ALTER TABLE items VALIDATE CONSTRAINT repeated;
```

## Related diagnostics

[relation_not_found](relation_not_found.md)
