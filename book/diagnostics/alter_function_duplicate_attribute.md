# alter_function_duplicate_attribute

## Code and severity

The error code is `alter_function_duplicate_attribute`.

## When it is reported

A PostgreSQL `ALTER FUNCTION` statement specifies volatility or parallel safety
more than once. Repeating the same value also triggers the error.

## What analysis knows and does not know

Analysis rejects the entire change and analyzes later statements using the
existing function definition.

## How to address it

Specify at most one volatility and one parallel-safety action per statement.

## Example

With a zero-argument function named `public.f` in the selected catalog:

```sql
ALTER FUNCTION public.f() STABLE IMMUTABLE;
```
