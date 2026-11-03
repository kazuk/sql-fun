# column_source_ambiguous

## Code and severity

Code: `column_source_ambiguous`. Severity: Error.

## When it is reported

A qualified column or whole-row reference matches more than one visible source.

## What analysis knows and does not know

The requested qualified reference is retained. The ambiguity concerns source
identity, before selecting a column within that source.

## How to address it

Assign distinct aliases and qualify the reference with the intended alias. Also
correct any duplicate-alias diagnostic.

## Example

PostgreSQL 18; selected schema contains `public.docs_items(id integer)`.

```sql
SELECT public.docs_items.id FROM public.docs_items CROSS JOIN public.docs_items;
```

Both FROM entries expose the same relation identity. Alias them `a` and `b`,
then use `a.id` or `b.id`. A duplicate-alias diagnostic can accompany this
error.

## Related diagnostics

[ambiguous_column](ambiguous_column.md) reports an unqualified column matches
multiple sources. [duplicate_alias](duplicate_alias.md) reports duplicate source
alias declarations.
