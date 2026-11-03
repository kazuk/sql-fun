# table_function_invalid_expression

## Code and severity

Code: `table_function_invalid_expression`. Severity: Error.

## When it is reported

A FROM function expression contains an aggregate, window function or an
illegally nested set-returning expression. A top-level set-returning function is
not rejected merely for returning a set.

## What analysis knows and does not know

The invalid-expression classification is retained, not a guarantee that the
entire invocation or its output types were otherwise resolved.

## How to address it

Move aggregation or window computation to an appropriate query level. Put
set-returning functions at a supported top-level FROM position instead of
nesting them inside another expression.

## Example

PostgreSQL builtin catalog containing abs(integer) and generate_series(integer,
integer).

```sql
SELECT * FROM ROWS FROM(abs(generate_series(1, 2)));
```

generate_series is nested inside abs. A direct FROM generate_series(1, 2) is a
different placement.

## Related diagnostics

[table_function_definition_not_allowed](table_function_definition_not_allowed.md)
concerns output definitions rather than expression placement.
[tablesample_expression](tablesample_expression.md) applies separate placement
rules to sampling inputs.
