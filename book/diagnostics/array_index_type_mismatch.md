# array_index_type_mismatch

## Code and severity

Code: `array_index_type_mismatch`. Severity: Error.

## When it is reported

A known array subscript type is neither int4 nor assignment-convertible to int4
in the selected catalog.

## What analysis knows and does not know

The subscript expression facts and its type are retained. The diagnostic
concerns the index type, not array bounds or dimensions.

## How to address it

Use an integer index or a supported conversion to int4. Check explicit input
hints against the intended index type.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. The named
placeholder is sql-fun input syntax. The conflicting hint can also produce
`parameter_constraint`.

```sql
SELECT (ARRAY[1, 2])[${index:bool}] AS n;
```

## Related diagnostics

[require_array](require_array.md) concerns the value being indexed;
[parameter_constraint](parameter_constraint.md) can report inconsistent
subscript input requirements.
