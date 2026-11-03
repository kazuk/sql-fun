# trigger_invalid_definition

## Code and severity

Code: `trigger_invalid_definition`. Severity: Error.

## When it is reported

A trigger target/name is missing or unsuitable, a trigger duplicates an existing
dialect-scoped identity, an event column is absent, a PostgreSQL trigger
function is unusable, or trigger options conflict.

## What analysis knows and does not know

The relevant target, trigger identity, event column, function or option detail
is retained. A function failure can mean missing, non-unique, wrong result type
or unsuitable view-row behavior; the payload may not distinguish those reasons.

## How to address it

Check the specific target/event/option explanation and the intended dialect.
PostgreSQL functions need an appropriate zero-input trigger signature and return
type. SQLite trigger namespaces, bodies and event rules differ.

## Example

PostgreSQL builtin-only example with no `public.items`: the target cannot
resolve. SQLite DROP TRIGGER uses different syntax; do not reuse the PostgreSQL
ON clause there.

```sql
DROP TRIGGER missing ON public.items;
```

## Related diagnostics

[rule_invalid_definition](rule_invalid_definition.md) concerns PostgreSQL rules;
[column_not_found_in_table](column_not_found_in_table.md) concerns ordinary
table-column lookup.
