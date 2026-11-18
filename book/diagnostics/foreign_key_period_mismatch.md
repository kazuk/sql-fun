# foreign_key_period_mismatch

## Code and severity

Code: `foreign_key_period_mismatch`. Severity: Error.

## When it is reported

An explicit PostgreSQL PERIOD foreign key column list marks only one side with
PERIOD.

## What analysis knows and does not know

This check uses the foreign key declaration. It does not inspect stored rows.

## How to address it

Mark the final referencing and referenced columns PERIOD.

## Example

```sql
CREATE TABLE child (a daterange, b daterange,
  FOREIGN KEY (a, PERIOD b) REFERENCES parent (a, b));
```

## Related diagnostics

[foreign_key_temporal_key_missing](foreign_key_temporal_key_missing.md)
