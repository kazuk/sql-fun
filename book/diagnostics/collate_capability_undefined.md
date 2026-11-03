# collate_capability_undefined

## Code and severity

Code: `collate_capability_undefined`. Severity: Error.

## When it is reported

A PostgreSQL declaration explicitly names a resolved collation, but the resolved
declared type has no known collation-capability fact.

## What analysis knows and does not know

The declared type is retained. Analysis cannot determine applicability; it has
not proved the type non-collatable. Expression COLLATE validation can leave
unknown capability unresolved without emitting this declaration diagnostic.

## How to address it

Check the selected catalog and the type declaration metadata. Supply supported
collation capability information or omit the explicit declaration annotation
only if the intended declaration permits that choice.

## Example

Illustrative PostgreSQL declaration: `custom_type` must resolve while its
collation capability is unavailable, and `C` must resolve. The normal builtin
text type has known capability and does not reproduce this condition.

```sql
CREATE TABLE annotated (value custom_type COLLATE "C");
```

## Related diagnostics

[collate_operand_type](collate_operand_type.md) establishes known
non-collatability; [column_type_not_found](column_type_not_found.md) means the
type itself was not resolved.
