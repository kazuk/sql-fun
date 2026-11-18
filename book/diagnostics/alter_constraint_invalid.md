# alter_constraint_invalid

## Code and severity

Code: `alter_constraint_invalid`. Severity: Error.

## When it is reported

The requested operation is incompatible with the matched constraint.

## What analysis knows and does not know

The analyzer uses the selected PostgreSQL catalog and the retained declarations.

## How to address it

Check the constraint kind and enforcement state before changing its attributes
or validating it.

## Example

```sql
ALTER TABLE items VALIDATE CONSTRAINT unique_key;
```

## Related diagnostics

[relation_not_found](relation_not_found.md)
