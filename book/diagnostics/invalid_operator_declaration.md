# invalid_operator_declaration

## Code and severity

The error code is `invalid_operator_declaration`.

## When it is reported

PostgreSQL analysis reports this error for invalid `CREATE OPERATOR` and
`ALTER OPERATOR … SET`
definitions. Examples include a missing right operand type, a missing
implementation function with the exact operand signature, a duplicate
operator signature, an attempt to disable a previously enabled flag, and a
planner attribute incompatible with the operand
count or return type.

## What analysis knows and does not know

Analysis checks types, routines, and operator signatures in the selected
catalog. A rejected declaration leaves the catalog unchanged. If a
selectivity estimator is missing from the bundled catalog, analysis cannot
validate the declaration even if PostgreSQL accepts it. See the
[CREATE OPERATOR reference](../sql-analysis/statements/create-operator.md).

## How to address it

Check the operand types and the implementation function's input signature.
Check the target schema and qualify references where necessary. When the
diagnostic identifies an estimator, check its signature and availability in
the selected catalog.

## Example

```sql
CREATE OPERATOR public.%% (
    RIGHTARG = bigint,
    FUNCTION = pg_catalog.int4um
);
```

`int4um` accepts `integer`. A declaration with `RIGHTARG = bigint` requires
an implementation that accepts `bigint` directly.

## Related diagnostics

[invalid_function_signature](invalid_function_signature.md) explains why
analysis can reject a function declaration.
