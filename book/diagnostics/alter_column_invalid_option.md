# alter_column_invalid_option

## Code and severity

Code: `alter_column_invalid_option`. Severity: Error.

## When it is reported

A SET column option lacks a value or its value is not an accepted finite number
at least -1. Underflow and subnormal values are also rejected.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The option name identifies the invalid entry; the diagnostic does not
establish an effective option value.

## How to address it

Supply a valid numeric value for the intended n_distinct or n_distinct_inherited
setting. Remove an option from a RESET list rather than attaching a replacement
value there.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET (n_distinct = -2);
```

The name is supported, but -2 is below the allowed range.

## Related diagnostics

[alter_column_reset_value](alter_column_reset_value.md) concerns a supplied
RESET value; [alter_column_unknown_option](alter_column_unknown_option.md)
concerns the name.
