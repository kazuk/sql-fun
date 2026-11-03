# truth_test_invalid_literal

## Code and severity

Code: `truth_test_invalid_literal`. Severity: Error.

## When it is reported

An unknown string literal in a PostgreSQL truth predicate is not accepted as
boolean input.

## What analysis knows and does not know

The literal value is retained. The analyzer applies PostgreSQL boolean spellings
and unambiguous abbreviations; a recognized text value is not automatically a
valid boolean literal.

## How to address it

Use an accepted boolean value such as true/false or write the intended
comparison. Avoid ambiguous abbreviations such as `o`, which could mean on or
off.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This is PostgreSQL
boolean input behavior, not a claim about SQLite truth-value conversion.

```sql
SELECT 'o' IS TRUE AS result;
```

## Related diagnostics

[truth_test_operand_type](truth_test_operand_type.md) concerns a known
non-boolean operand type; [query_parse](query_parse.md) concerns SQL grammar
rather than literal value conversion.
