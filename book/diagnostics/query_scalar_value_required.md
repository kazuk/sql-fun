# query_scalar_value_required

## Code and severity

Code: `query_scalar_value_required`. Severity: Error.

## When it is reported

A scalar consumer receives an internal list, or a SQLite row constructor where a
scalar is required.

## What analysis knows and does not know

The expression shape is known to be non-scalar. This does not prohibit supported
row-to-row comparisons or PostgreSQL composite values.

## How to address it

Supply one value to the scalar operation, or use a supported row comparison
where a row is intended.

## Example

Illustrative SQLite SQLAST example; no schema is needed. This is validated by
the maintained SQLite row-consumer tests, not the PostgreSQL-only installed
initialization path.

```sql
SELECT CAST((1, 2) AS TEXT);
```

A two-field row cannot be cast as a single text scalar in this SQLite context.
PostgreSQL has different composite-value rules.

## Related diagnostics

[membership_width](membership_width.md) reports a row-membership width mismatch.
[query_invalid_star_projection_placement](query_invalid_star_projection_placement.md)
reports field expansion is in a scalar position.
