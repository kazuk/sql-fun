# alter_column_unknown_option

## Code and severity

Code: `alter_column_unknown_option`. Severity: Error.

## When it is reported

SET column options contains a name other than n_distinct or
n_distinct_inherited.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The rejected option name is retained. This check applies to SET;
RESET has different handling of unknown names.

## How to address it

Check that the option is a supported per-column attribute, rather than a table
or index storage option. Correct or remove the unknown name.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET (unknown_option = 1);
```

The value is numeric, but the option name is not supported.

## Related diagnostics

[alter_column_invalid_option](alter_column_invalid_option.md) concerns the
value; [alter_column_duplicate_option](alter_column_duplicate_option.md)
concerns repeated SET entries.
