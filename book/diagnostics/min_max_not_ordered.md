# min_max_not_ordered

## Code and severity

Code: `min_max_not_ordered`. Severity: Error.

## When it is reported

The common type of GREATEST/LEAST operands is known not to have the required
PostgreSQL comparison implementation.

## What analysis knows and does not know

The selected result type is retained. Common-type selection succeeded; the
failure concerns ordering capability.

## How to address it

Use data with defined ordering or an explicit supported conversion that gives
the ordering you intend. For JSON data, choose the intended representation
rather than assuming every JSON type has the same ordering rules.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT LEAST('{}'::json, '{}'::json);
```

## Related diagnostics

[min_max_missing_comparison_facts](min_max_missing_comparison_facts.md) has
unknown capability; [min_max_incompatible_types](min_max_incompatible_types.md)
cannot establish a common operand type.
