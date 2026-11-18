# temporal_key_too_short

## Code and severity

Code: `temporal_key_too_short`. Severity: Error.

## When it is reported

A PostgreSQL temporal primary or unique key has only a WITHOUT OVERLAPS column.

## What analysis knows and does not know

A temporal key requires at least one equality column before the period column.

## How to address it

Add the columns that identify rows for overlap checking before the period.

## Example

```sql
CREATE TABLE invalid_period (span daterange, PRIMARY KEY (span WITHOUT OVERLAPS));
```

## Related diagnostics

[temporal_period_type](temporal_period_type.md)
