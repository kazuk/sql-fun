# ORDER BY

## Overview

Ordinary SELECT ordering resolves standalone output names and one-based output
positions, then applicable source expressions. PostgreSQL output aliases take
precedence over source columns for a standalone ORDER BY name; GROUP BY has
[different precedence](group-by.md). Larger expressions resolve their own source
references rather than substituting output aliases.

```sql
SELECT a + 1 AS shifted FROM (SELECT 1 AS a) AS source ORDER BY shifted;
```

ORDER BY accepts ascending/descending direction and explicit NULLS ordering.
Analysis resolves supported PostgreSQL USING operators. Operand expressions and
USING operators contribute parameter constraints. It does not execute a sort,
establish runtime collation behavior, or guarantee a deterministic order from
tied keys.

## Dialect and engine-specific behavior

PostgreSQL compound-query ORDER BY refers to compound output names or ordinals,
not arbitrary new expressions or an operand's source scope. SQLite additionally
matches output expressions/aliases from either operand under its compound-order
rules. The final trailing clauses belong to the compound result; operand-local
clauses stay with their operands.

Window/aggregate ordering uses its own expression scope, not final SELECT output
alias rules; see [WINDOW](window.md) and
[function calls](../expressions/function-calls.md). Binding names successfully
does not establish complete aggregate, window or SQL ordering legality checking.

## Diagnostics

- [query_position_out_of_range](../../diagnostics/query_position_out_of_range.md):
  an output position is zero or outside the result list.
- [query_invalid_compound_order](../../diagnostics/query_invalid_compound_order.md):
  a compound ORDER BY item cannot identify an output under its dialect rules.
- [Name-resolution diagnostics](../expressions/names.md#diagnostics): missing or
  ambiguous columns/output aliases.
- [query_reference_not_found](../../diagnostics/query_reference_not_found.md) and
  [query_reference_ambiguous](../../diagnostics/query_reference_ambiguous.md):
  a named USING operator has no unique binding.

Operation and [parameter diagnostics](../expressions/parameters.md#diagnostics)
can also arise from ordering expressions.
