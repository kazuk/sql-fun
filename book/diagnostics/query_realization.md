# query_realization

## Code and severity

Code: `query_realization`. Severity: Error.

## When it is reported

Input placeholders, source mapping, or an explicitly supplied reusable CTE
catalog cannot be rendered into the statement that analysis requires.

## What analysis knows and does not know

The realization-stage explanation is retained. The failure precedes a completed
statement analysis; it is not a type-overload verdict.

## How to address it

Check placeholder consistency and the explicitly selected CTE definitions and
imports. Use named placeholders or native numbered parameters consistently
rather than mixing them.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This mixes native
and named placeholders in one input. The realization path can also report
CTE-specific failures that need the selected catalog to reproduce.

```sql
SELECT $1, ${value};
```

## Related diagnostics

[invalid_parameter_hint](invalid_parameter_hint.md) concerns hint
binding/resolution; [query_parse](query_parse.md) concerns parsing the rendered
SQL.
