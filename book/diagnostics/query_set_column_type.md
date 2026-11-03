# query_set_column_type

## Code and severity

Code: `query_set_column_type`. Severity: Error.

## When it is reported

A positional output of a set operation cannot obtain a legal common type.

## What analysis knows and does not know

The operation, output position and underlying common-type problem are retained.
Width may already match; missing metadata and proven incompatibility remain
different causes in the message.

## How to address it

Match corresponding output types using compatible expressions or an intended
explicit conversion. For missing type facts, check the selected catalog instead
of guessing a type.

## Example

PostgreSQL 18 builtin catalog. SQLite uses different dynamic-typing rules.

```sql
SELECT 1 UNION SELECT true;
```

An integer and a boolean do not combine as this UNION output. Use compatible
outputs when that matches the intended result.

## Related diagnostics

[query_set_column_count](query_set_column_count.md) reports operand widths
differ. [min_max_incompatible_types](min_max_incompatible_types.md) reports the
common-type failure occurs in GREATEST/LEAST.
