# ALTER VIEW

## Overview

PostgreSQL view column defaults can be set or removed for subsequent write
analysis:

```sql
ALTER VIEW public.item_names ALTER COLUMN label SET DEFAULT 'unnamed';
ALTER VIEW public.item_names ALTER COLUMN label DROP DEFAULT;
```

The target must be an ordinary view and the default column must exist. Changes
are validated before the view is updated; an invalid column leaves the view
unchanged. Effective view defaults override underlying defaults for the writable
columns described at [CREATE VIEW](create-view.md#writable-views). Defaults are
analyzed expressions, not executed values. Ownership changes use [ALTER … OWNER
TO](alter-owner.md).

## Dialect and engine-specific behavior

This describes implemented PostgreSQL view-default behavior. Enable/disable
trigger or rule ALTER VIEW operations are rejected by the view alteration path;
broader ALTER VIEW operations remain incomplete. Do not infer support for SQLite
ALTER VIEW or arbitrary PostgreSQL view alterations from CREATE VIEW support.

Non-owner ALTER VIEW semantics remain incomplete
([#544](https://github.com/kazuk/sql-fun/issues/544)). SET/RESET view options do
not have completed semantic application; the implemented SET/DROP column-default
operations above are exceptions to the older blanket non-owner limitation.
Recognizing other ALTER VIEW syntax does not establish a catalog transition.

## Diagnostics

- [view_not_found](../../diagnostics/view_not_found.md): the requested view is
  absent on an applicable lookup path.
- [view_unknown_default_column](../../diagnostics/view_unknown_default_column.md):
  SET/DROP DEFAULT refers to a missing view output column.
- [view_invalid_alter](../../diagnostics/view_invalid_alter.md): a checked view
  alteration such as enabling/disabling a trigger or rule is invalid.

Default expressions can emit the shared expression diagnostics.
