# function_not_found

## Code and severity

Code: `function_not_found`. Severity: Error.

## When it is reported

No function definition can be found for a scalar function name in the selected
catalog/search path.

## What analysis knows and does not know

The requested function name and available name span are retained. This does not
prove the function is absent from the live database or an unloaded extension.

## How to address it

Check the name, qualification, selected schema and required extension
definitions. If the function exists, check that its catalog definition is
included.

## Example

PostgreSQL 18 builtin catalog; `missing_function` is absent. Function
availability differs across engines and versions.

```sql
SELECT missing_function(1);
```

Load or reference the intended existing function; do not assume an unknown
function name makes its SQL syntax invalid.

## Related diagnostics

[function_overload_resolution_failed](function_overload_resolution_failed.md)
reports named definitions exist but argument matching fails.
[table_function_not_found](table_function_not_found.md) reports a FROM function
source is missing.
