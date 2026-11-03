# implicit_cast_not_found

## Code and severity

Code: `implicit_cast_not_found`. Severity: Error.

## When it is reported

An expression with a known type needs a specific result type, but the catalog
provides no required implicit conversion.

## What analysis knows and does not know

The source and target types are retained. Explicit or assignment conversion
availability does not establish an implicit conversion.

## How to address it

Use an expression with the required type or an intentional supported explicit
conversion. Check the surrounding construct for the expected type; do not assume
all cast contexts permit the same conversions.

## Example

PostgreSQL schema input: `items` and a valid zero-input `trigger_function`
returning trigger must already be loaded. A typed non-boolean WHEN operand needs
an implicit boolean conversion; another trigger diagnostic can accompany this
failure.

```sql
CREATE TRIGGER guarded BEFORE UPDATE ON items
FOR EACH ROW WHEN (1) EXECUTE FUNCTION trigger_function();
```

## Related diagnostics

[explicit_cast_not_defined](explicit_cast_not_defined.md) concerns requested
explicit casts;
[function_arg_implicit_cast_not_found](function_arg_implicit_cast_not_found.md)
concerns a selected function argument.
