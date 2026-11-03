# tablesample_expression

## Code and severity

Code: `tablesample_expression`. Severity: Error.

## When it is reported

A TABLESAMPLE argument or REPEATABLE seed contains an aggregate, window or
set-returning function at a prohibited query level. Nested query boundaries
affect which expressions belong to the sampling clause.

## What analysis knows and does not know

The diagnostic identifies TABLESAMPLE or REPEATABLE placement. It does not imply
that every function or every subquery is forbidden in sampling inputs.

## How to address it

Move the computation to a supported query level or supply a scalar expression
permitted in the sampling clause. Keep type validation separate.

## Example

PostgreSQL builtin catalog containing sum and project table `public.t(a
integer)`.

```sql
SELECT * FROM public.t TABLESAMPLE system(sum(1));
```

A direct aggregate is invalid in this position. The ordinary CLI can also report
tablesample_metadata_unavailable.

## Related diagnostics

[tablesample_argument_type](tablesample_argument_type.md) concerns coercion;
[table_function_invalid_expression](table_function_invalid_expression.md)
concerns a different FROM-function placement rule.
