# relation_already_exists

## Code and severity

Code: `relation_already_exists`. Severity: Error.

## When it is reported

A relation rename in the retained ALTER command analysis targets a name already
occupied by a table.

## What analysis knows and does not know

The destination relation name is retained. Other create/rename paths can instead
report the broader `conflicting_creation` diagnostic.

## How to address it

Choose an unused destination appropriate to the selected schema or correct the
intended target. Do not remove an existing relation without checking its
purpose.

## Example

Illustrative PostgreSQL-style rename input; selected schema already contains
`docs_items(id integer)`. The exact code depends on the rename analysis path;
normal SQL conversion may use `conflicting_creation` instead.

```sql
ALTER TABLE docs_items RENAME TO docs_items;
```

The requested destination is already occupied. This illustrates the collision;
it is not a guarantee that every rename path emits this specific code.

## Related diagnostics

[conflicting_creation](conflicting_creation.md) reports a broader catalog
identity collision. [relation_not_found](relation_not_found.md) reports the
source relation cannot be found.
