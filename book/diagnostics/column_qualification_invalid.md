# column_qualification_invalid

## Code and severity

Code: `column_qualification_invalid`. Severity: Error.

## When it is reported

A column reference uses qualification that is invalid for the selected dialect;
the current producer rejects a database/catalog component for SQLite column
lookup.

## What analysis knows and does not know

The qualifier and column name are retained. No valid source binding or column
type was established from that qualification.

## How to address it

Use the selected dialect's supported qualification and visible aliases. Do not
interpret PostgreSQL database/schema/table spelling as SQLite spelling.

## Example

Illustrative SQLite SQLAST input; `docs_items(id integer)` exists. The
PostgreSQL-only installed schema initialization path does not validate this
SQLite example.

```sql
SELECT sample.public.docs_items.id FROM docs_items;
```

This four-part PostgreSQL-style reference is not a supported SQLite column
qualification. An alias such as `t.id` avoids that form.

## Related diagnostics

[column_database_name_required](column_database_name_required.md) reports
PostgreSQL needs an explicit database identity.
[column_source_not_found](column_source_not_found.md) reports valid
qualification has no matching source.
