# alter_column_reset_value

## Code and severity

Code: `alter_column_reset_value`. Severity: Error.

## When it is reported

A RESET column-option entry supplies a value.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The option name is retained. RESET removes a setting; it does not
assign a replacement value.

## How to address it

Remove the value from RESET, or use SET when assigning an explicit value is
intended.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a RESET (n_distinct = 1);
```

Use RESET (n_distinct) to remove the setting.

## Related diagnostics

[alter_column_invalid_option](alter_column_invalid_option.md) validates values
supplied to SET.
