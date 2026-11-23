# CREATE CAST

Schema analysis supports PostgreSQL casts declared with `WITH FUNCTION`,
`WITHOUT FUNCTION`, and `WITH INOUT`. Declare the source and target types and
any referenced function before declaring the cast. Subsequent statements can use
the cast in the same direction.

- A cast with no context clause is available in explicit cast expressions.
- `AS ASSIGNMENT` also permits conversion when assigning a value to a column.
- `AS IMPLICIT` also permits conversion during expression and routine resolution.

A cast declared on a domain is recorded for duplicate detection. Subsequent
coercion follows the domain's base type, and analysis reports a warning.

When a command also renders a standalone `CREATE CAST` declaration as SQL,
it reports [`query_deparse`](../../diagnostics/query_deparse.md).
Rendering this statement is tracked in
[#1243](https://github.com/kazuk/sql-fun/issues/1243).

Result nullability currently follows the input expression. A cast function
can return `NULL` for a non-null input, even when it is declared `STRICT`.
This case is tracked in [#1244](https://github.com/kazuk/sql-fun/issues/1244).
