# truth_test_operand_type

## Code and severity

Code: `truth_test_operand_type`. Severity: Error.

## When it is reported

An IS TRUE, IS FALSE or IS UNKNOWN predicate receives an independently known
operand type that cannot be implicitly converted to PostgreSQL boolean.

## What analysis knows and does not know

The operand type is retained. This is different from an unknown string literal
being interpreted through PostgreSQL boolean input rules.

## How to address it

Use a boolean expression such as the intended comparison. Do not assume
PostgreSQL treats every nonzero integer as a boolean predicate.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. SQLite truth
predicates have different coercion semantics.

```sql
SELECT 1 IS TRUE AS result;
```

## Related diagnostics

[truth_test_invalid_literal](truth_test_invalid_literal.md) concerns unknown
literal input; [implicit_cast_not_found](implicit_cast_not_found.md) concerns
other required implicit conversions.
