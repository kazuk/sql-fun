# expression_type_not_known

## Code and severity

Code: `expression_type_not_known`. Severity: Error.

## When it is reported

Binary-operator resolution reaches an operand whose type cannot be obtained, so
normal typed overload selection cannot proceed.

## What analysis knows and does not know

The operand expression facts are retained, including available kind, type and
nullability. The missing type is not evidence that no overload exists for the
intended type.

## How to address it

Address earlier name or type diagnostics first and provide supported SQL type
context where needed. If information remains unavailable, report the analysis
limitation with the original query.

## Example

Illustrative PostgreSQL expression. The operand must remain unresolved when the
typed operator resolver is reached. An ordinary missing column can be diagnosed
earlier without emitting this additional code; this SQL is not claimed as a
standalone reproducer.

```sql
SELECT unknown_expression + 1;
```

## Related diagnostics

[undefined_operator](undefined_operator.md) concerns an unusable operator
expression; [parameter_type_unresolved](parameter_type_unresolved.md) concerns
final application input binding.
