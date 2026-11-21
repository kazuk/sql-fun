# drop_function_dependency

## Code and severity

Code: `drop_function_dependency`. Severity: Error.

## When it is reported

A PostgreSQL `DROP FUNCTION` with default or explicit `RESTRICT` targets a
function used by a view or materialized view.

## What analysis knows and does not know

The diagnostic identifies a dependent view in the selected catalog. The
statement leaves the function and its dependents available to later statements.
For the scope of dependencies checked, see [DROP FUNCTION in the SQL Analysis
Reference](../sql-analysis/statements.md#other-postgresql-utility-statements).

## How to address it

Use `CASCADE` if the dependent views should also be removed.

## Example

Load these PostgreSQL declarations into the analyzed schema, then analyze the
drop statement against that schema.

```sql
CREATE FUNCTION public.example() RETURNS integer LANGUAGE sql AS 'SELECT 1';
CREATE VIEW public.example_view AS SELECT public.example() AS value;
```

```sql
DROP FUNCTION public.example() RESTRICT;
```

## Related diagnostics

[function_not_found](function_not_found.md) reports a missing function. A
dependency error instead means the target was found and the removal was
rejected.
