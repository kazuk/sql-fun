# cross_database_operator_reference

## Code and severity

Code: `cross_database_operator_reference`. Severity: Error.

## When it is reported

A PostgreSQL operator reference has three or more name components. PostgreSQL
accepts these forms in its parser but rejects them during semantic analysis.
Three components request database qualification; longer forms have excessive
qualification.

## What analysis knows and does not know

The requested components are known. The operator remains unresolved; analysis
does not look up another database or substitute an unqualified operator.

## How to address it

Use an operator in the selected database, with at most its schema and operator
name. Select the intended datasource explicitly if the query belongs elsewhere.

## Example

With a PostgreSQL 17 or 18 builtin catalog, this input reports the diagnostic:

```sql
SELECT 1 OPERATOR(a.b.+) 2;
```

To select the builtin addition operator in the current database:

```sql
SELECT 1 OPERATOR(pg_catalog.+) 2;
```

The current CLI also reports `query_deparse` for this example because its SQL
renderer cannot reproduce operator references with database qualification. The
semantic diagnostic remains distinct from that rendering limitation.

## Related diagnostics

[operator_name_syntax](operator_name_syntax.md) identifies malformed component
shapes rather than unsupported database qualification.
[undefined_operator](undefined_operator.md) reports an operator that cannot be
resolved or used with the selected operands.

[query_deparse](query_deparse.md) reports the separate SQL rendering limitation.
