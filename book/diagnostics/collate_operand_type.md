# collate_operand_type

## Code and severity

Code: `collate_operand_type`. Severity: Error.

## When it is reported

An explicit COLLATE annotation is applied to a type that the catalog establishes
as non-collatable. This can occur in an expression or a supported declaration.

## What analysis knows and does not know

The resolved operand or declared type is retained. A known false collation
capability differs from unavailable capability metadata.

## How to address it

Remove the annotation from non-collatable data, or apply it to the intended
collatable text expression. Do not cast non-text data merely to suppress the
message without checking the desired semantics.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This operand
capability rule is PostgreSQL-specific; SQLite uses different collation rules.

```sql
SELECT 1 COLLATE "C" AS value;
```

## Related diagnostics

[collate_capability_undefined](collate_capability_undefined.md) has unavailable
declaration metadata; [collation_not_found](collation_not_found.md) concerns
name lookup.
