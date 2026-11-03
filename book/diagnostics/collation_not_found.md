# collation_not_found

## Code and severity

Code: `collation_not_found`. Severity: Error.

## When it is reported

An explicit collation name cannot be resolved in the selected catalog and search
path.

## What analysis knows and does not know

The requested collation is retained. Failure to find catalog metadata does not
prove that the collation is absent from the live database.

## How to address it

Check the spelling and qualification and ensure the selected catalog includes
the intended collation. Use a collation that actually exists for the selected
engine.

## Example

PostgreSQL 18 builtin catalog; `missing_collation` is absent. SQLite collation
binding has separate parser/catalog constraints.

```sql
SELECT 'a' COLLATE missing_collation;
```

The operand is text; the failure concerns the collation lookup, not text
applicability.

## Related diagnostics

[collation_name_ambiguous](collation_name_ambiguous.md) reports multiple
matching collations. [collate_operand_type](collate_operand_type.md) reports a
resolved collation cannot apply to the operand.
