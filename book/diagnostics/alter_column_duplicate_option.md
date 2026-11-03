# alter_column_duplicate_option

## Code and severity

Code: `alter_column_duplicate_option`. Severity: Error.

## When it is reported

A SET column-option list repeats the same option name.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The repeated name is retained. RESET lists do not use this
duplicate-SET check.

## How to address it

Specify each option once with the intended value.

## Example

PostgreSQL; the selected schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET (n_distinct = 1, n_distinct = 2);
```

Both entries have the same name; the analyzer does not silently choose the last
one.

## Related diagnostics

[alter_column_unknown_option](alter_column_unknown_option.md) concerns names
outside the supported set.
