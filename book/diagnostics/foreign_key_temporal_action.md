# foreign_key_temporal_action

## Code and severity

Code: `foreign_key_temporal_action`. Severity: Error.

## When it is reported

A PostgreSQL PERIOD foreign key specifies an ON UPDATE or ON DELETE action other
than NO ACTION.

## What analysis knows and does not know

This check uses the foreign key declaration. It does not inspect stored rows.

## How to address it

Use NO ACTION or omit the action.

## Example

```sql
CREATE TABLE child (a daterange, b daterange,
  FOREIGN KEY (a, PERIOD b) REFERENCES parent ON DELETE CASCADE);
```

## Related diagnostics

[foreign_key_temporal_key_missing](foreign_key_temporal_key_missing.md)
