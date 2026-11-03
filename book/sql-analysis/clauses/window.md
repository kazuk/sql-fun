# WINDOW and OVER

## Overview

Window analysis retains named definitions, OVER references, inherited window
references, PARTITION BY, window ordering, frame modes/bounds and exclusions.
References record successful, missing or ambiguous lookup. Definition expressions,
function modifiers and frame offsets participate in parameter analysis.

```sql
SELECT sum(1) OVER w
WINDOW w AS (ORDER BY 1 ROWS BETWEEN $1 PRECEDING AND CURRENT ROW);
```

Window ORDER BY expressions use source names; the query's final ORDER BY alias
and output-ordinal rules do not apply inside a window. PostgreSQL ROWS/GROUPS
frame offsets require bigint-compatible inputs. For a known supported RANGE
ordering type, temporal ordering constrains offsets to interval; builtin numeric
or interval ordering constrains offsets to its applicable type. Inherited
ordering information participates in these constraints.

## Dialect and engine-specific behavior

PostgreSQL window definitions inherit from preceding definitions; duplicate named
definitions are diagnosed. SQLite uses its own window-name binding rules,
including retained forward inheritance outcomes, and window definitions do not
gain SELECT projection aliases. Do not infer identical diagnostic behavior for
missing SQLite inherited names from PostgreSQL behavior.

Parameter-dependent RANGE ordering and custom RANGE operator classes are not
fully analyzed: custom in_range metadata is not generally available. Retaining a
frame, mode or exclusion does not establish complete frame, aggregate or window
legality checking. SQLite offsets do not acquire PostgreSQL parameter constraints.
Analysis describes metadata without evaluating the window's rows.

## Diagnostics

- [query_duplicate_window_definition](../../diagnostics/query_duplicate_window_definition.md):
  repeated PostgreSQL named definitions.
- [query_reference_not_found](../../diagnostics/query_reference_not_found.md) and
  [query_reference_ambiguous](../../diagnostics/query_reference_ambiguous.md):
  a checked window/operator reference has no unique binding.
- [query_invalid_grouping_placement](../../diagnostics/query_invalid_grouping_placement.md):
  GROUPING is used at a prohibited frame-offset query level.
- [parameter_constraint](../../diagnostics/parameter_constraint.md): incompatible
  requirements from frame offsets or other occurrences of an input.

PARTITION/ORDER/frame operands also use their ordinary
[name](../expressions/names.md#diagnostics),
[function](../expressions/function-calls.md#diagnostics) and operator diagnostics.
