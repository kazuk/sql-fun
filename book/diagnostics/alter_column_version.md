# alter_column_version

## Code and severity

Code: `alter_column_version`. Severity: Error.

## When it is reported

SET STATISTICS DEFAULT is used with an explicitly selected PostgreSQL version
earlier than 17.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. The selected version, rather than the version of an ambient database,
controls this capability check.

## How to address it

For PostgreSQL 15–16, use SET STATISTICS -1 to restore the default. Select a
newer version only if that is the actual analysis target.

## Example

PostgreSQL 15–16 selected; the project schema contains `public.t(a integer)`.

```sql
ALTER TABLE public.t ALTER COLUMN a SET STATISTICS DEFAULT;
```

PostgreSQL 17–18 support this spelling. The numeric -1 form is distinct from an
invalid value below -1.

## Related diagnostics

[alter_column_statistics_too_low](alter_column_statistics_too_low.md) reports
out-of-range negative targets.
