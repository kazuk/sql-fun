# invalid_parameter_hint

## Code and severity

Code: `invalid_parameter_hint`. Severity: Error.

## When it is reported

Placeholder bindings or explicit input hints cannot be constructed or resolved,
including an unknown hinted type or inconsistent hints.

## What analysis knows and does not know

The hint-stage explanation is retained. The analyzer has not established a
usable hinted input binding for the statement.

## How to address it

Use supported `${name}` or `${name:type}` input syntax with a resolvable type
identifier. Keep repeated hints compatible and use only the shortest sufficient
input type.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. `missing_type` must
not exist in the selected schema. Named placeholders are sql-fun syntax, not
PostgreSQL server syntax.

```sql
SELECT ${value:missing_type};
```

## Related diagnostics

[parameter_constraint](parameter_constraint.md) concerns hints conflicting with
SQL constraints; [query_realization](query_realization.md) concerns
rendering/expansion before statement analysis.
