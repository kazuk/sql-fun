# GROUP BY

## Overview

Analysis supports grouping expressions, empty grouping sets, ROLLUP, CUBE and
GROUPING SETS within the legality-checking limits below. PostgreSQL standalone
output aliases and one-based output ordinals can refer to SELECT targets. When a
GROUP BY name matches an input column and an output alias, the input column
takes precedence. Aliases/ordinals are not a substitute for source-name
resolution inside a larger expression.

```sql
SELECT a + 1 AS shifted, GROUPING(a + 1)
FROM (SELECT 1 AS a) AS source
GROUP BY shifted;
```

GROUPING arguments are bound to grouping keys of their owning query, retaining
argument order, duplicates and correlated-query ownership. Matching uses
analyzed expressions, including resolved aliases/ordinals and no-op casts; it
does not execute the expression. Analysis reports a non-null integer result.
GROUPING permits at most 31 arguments. Its argument matching and placement
validation are distinct from ordinary function overload resolution.

All supported grouping expressions contribute parameter constraints. See
[parameter analysis](../expressions/parameters.md) for input versus result type
alternatives.

## Dialect and engine-specific behavior

SQLite grouping expressions and applicable output aliases participate in name
resolution, without importing PostgreSQL common-type/boolean requirements.
PostgreSQL GROUPING, ROLLUP/CUBE and grouping-set behavior above does not
establish SQLite support for those forms. Full aggregate and grouping legality
checking remains incomplete; successful key binding is not a complete SQL
validator.

## Diagnostics

- [query_position_out_of_range](../../diagnostics/query_position_out_of_range.md):
  an ordinal does not identify a SELECT output.
- [Name-resolution diagnostics](../expressions/names.md#diagnostics): missing or
  ambiguous grouping columns/aliases.
- [query_grouping_argument_not_grouped](../../diagnostics/query_grouping_argument_not_grouped.md):
  a GROUPING argument does not match a grouping key of its owning query.
- [query_invalid_grouping_placement](../../diagnostics/query_invalid_grouping_placement.md):
  prohibited placement/nesting, including GROUPING inside its own GROUP BY,
  directly or through an alias/ordinal, or within incompatible aggregate scopes.
- [query_too_many_grouping_arguments](../../diagnostics/query_too_many_grouping_arguments.md):
  more than 31 GROUPING arguments.

Grouping operands keep their expression and parameter diagnostics as well.
