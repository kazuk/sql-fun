# temporal_period_type

## Code and severity

Code: `temporal_period_type`. Severity: Error.

## When it is reported

A PostgreSQL 18 temporal key declares WITHOUT OVERLAPS on a column whose type is
neither a range nor a multirange.

## What analysis knows and does not know

The analyzer checks the declared column type using the selected catalog.

## How to address it

Use a range or multirange data type for the final temporal key column.

## Example

```sql
CREATE TABLE invalid_period (b integer, PRIMARY KEY (b WITHOUT OVERLAPS));
```

## Related diagnostics

[foreign_key_temporal_key_missing](foreign_key_temporal_key_missing.md)
