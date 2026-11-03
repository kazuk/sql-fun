# min_max_empty

## Code and severity

Code: `min_max_empty`. Severity: Error.

## When it is reported

An analyzed GREATEST or LEAST expression has no operands.

## What analysis knows and does not know

The empty operand list is known. No common result type or comparison capability
can be selected.

## How to address it

Supply at least one intended operand. Do not rely on a default value being
inferred for an empty call.

## Example

Illustrative PostgreSQL syntax for an empty operand list. The PostgreSQL parser
currently rejects this SQL earlier with `query_parse`; `min_max_empty`
identifies the corresponding semantic condition if an empty operand list reaches
analysis. SQLite scalar min/max functions are different constructs.

```sql
SELECT GREATEST();
```

## Related diagnostics

[query_parse](query_parse.md) can preempt semantic analysis;
[min_max_incompatible_types](min_max_incompatible_types.md) has operands with
incompatible types.
