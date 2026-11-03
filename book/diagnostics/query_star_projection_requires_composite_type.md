# query_star_projection_requires_composite_type

## Code and severity

Code: `query_star_projection_requires_composite_type`. Severity: Error.

## When it is reported

A projection attempts row-field expansion from a value that is not known to have
a composite type.

## What analysis knows and does not know

The expansion was recognized, but analysis cannot expose fields from that
operand. A source with one scalar column does not become a composite value
merely through whole-row syntax.

## How to address it

Expand a known row/composite value or project the scalar directly. Check the
selected catalog if a composite definition should have been available.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed.

```sql
SELECT (1).*;
```

`1` is scalar. `SELECT 1` projects it directly; `SELECT (ROW(1)).*` expands an
actual row.

## Related diagnostics

[query_invalid_star_projection_placement](query_invalid_star_projection_placement.md)
reports expansion is outside a projection target.
[member_access_requires_composite_type](member_access_requires_composite_type.md)
reports ordinary field access needs a composite operand.
