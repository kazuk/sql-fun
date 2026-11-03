# view_invalid_alter

## Code and severity

Code: `view_invalid_alter`. Severity: Error.

## When it is reported

A PostgreSQL view alteration attempts to enable or disable trigger or rule state
using a prohibited ALTER operation.

## What analysis knows and does not know

The invalid alteration is retained as a view diagnostic. This does not establish
that all trigger/rule declarations on views are invalid.

## How to address it

Use an operation supported for the intended view behavior. Check the distinction
between declaring an appropriate rule/trigger and changing state through this
ALTER spelling.

## Example

PostgreSQL; the selected project schema contains ordinary view public.v.

```sql
ALTER VIEW public.v DISABLE RULE r;
```

The state-change operation is rejected independently of whether a rule named r
exists. SQLite does not support this PostgreSQL syntax.

## Related diagnostics

[rule_invalid_definition](rule_invalid_definition.md) and
[trigger_invalid_definition](trigger_invalid_definition.md) concern declaration
semantics, not this ALTER operation.
