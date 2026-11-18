# foreign_key_period_mismatch

## Code and severity

Code: `foreign_key_period_mismatch`. Severity: Error.

## When it is reported

A foreign key omits PERIOD when defaulting to a temporal primary key, or marks
only one side when both column lists are explicit.

## What analysis knows and does not know

This check uses the foreign key declaration. It does not inspect stored rows.

## How to address it

Mark the final referencing column PERIOD. When the referenced column list is
explicit, mark its final column PERIOD as well.

## Example

```sql
CREATE TABLE child (a daterange, b daterange,
  FOREIGN KEY (a, PERIOD b) REFERENCES parent (a, b));
```

## Related diagnostics

[foreign_key_temporal_key_missing](foreign_key_temporal_key_missing.md)
