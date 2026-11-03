# rule_invalid_definition

## Code and severity

Code: `rule_invalid_definition`. Severity: Error.

## When it is reported

A PostgreSQL rule target or rule identity is missing/unsuitable, a creation
duplicates an existing relation-scoped rule, or a rule-specific legality
condition fails.

## What analysis knows and does not know

The target, rule identity or definition detail is retained. Some target failures
combine missing and unsupported target kinds, so the payload may not establish a
narrower cause.

## How to address it

Check the target relation, rule name and statement-specific detail. Use
supported rule targets and comply with event/action restrictions rather than
assuming any syntactically accepted rule can be applied.

## Example

PostgreSQL builtin-only example with no `public.items`: the target cannot
resolve. It also reports a deparse limitation. If the table exists, the rule
name must still identify an effective rule on that table. Rules are
PostgreSQL-specific.

```sql
DROP RULE missing ON public.items;
```

## Related diagnostics

[trigger_invalid_definition](trigger_invalid_definition.md) concerns trigger
rules; [conflicting_creation](conflicting_creation.md) concerns general creation
namespaces.
