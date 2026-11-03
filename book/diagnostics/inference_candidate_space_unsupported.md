# inference_candidate_space_unsupported

## Code and severity

Code: `inference_candidate_space_unsupported`. Severity: Error.

## When it is reported

A type-inference operation would require an unsupported catalog-wide search over
possible types, rather than the bounded candidates available to the analyzer.

## What analysis knows and does not know

The operation description is retained. The diagnostic establishes an analysis
limitation, not incompatible SQL types or invalid database syntax. Other
diagnostics, including unresolved input bindings, may accompany it.

## How to address it

Express the intended operand types using supported SQL constraints or explicit
input type hints. Avoid guessing a type from the result alone. If the intended
query still needs unsupported analysis, retain that limitation when reporting
it; changing valid SQL is not proof of a database error.

## Example

PostgreSQL 17–18 builtin schema.

```sql
SELECT ($1)[1] IS DISTINCT FROM ($2)[1] AS result;
```

The analyzer cannot search every possible array input type. Supplying known
operand types through supported expressions can avoid that search. This example
also reports `parameter_type_unresolved` for both input bindings; those must be
resolved independently before the statement is complete.

## Related diagnostics

[parameter_type_unresolved](parameter_type_unresolved.md) concerns the final
application binding; [parameter_constraint](parameter_constraint.md) concerns
incompatible requirements, rather than an unsupported search.
