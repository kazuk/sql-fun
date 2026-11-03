# Arrays and rows

## Overview

### Row constructors

`ROW(...)` and implicit rows such as `(1, true)` retain their fields in SQL
order, including nested rows. PostgreSQL also permits `ROW()` and `ROW(1)`.
Anonymous fields are named `f1`, `f2`, and so on:

```sql
SELECT (ROW(1, true)).f1, (ROW(1, true)).*
```

A row constructor produces one non-null row value even when some or all fields
are NULL. For row-valued `IS NULL` /`IS NOT NULL`, PostgreSQL tests all fields;
a mixed-null row can satisfy neither predicate. The constructor's implicit
output name is `row`; use `AS` to choose another name. Field projection reports
each field's own type and nullability. Anonymous row results have no named
result type in the analysis API; project fields when you need individual
result-column types.

Qualified `ROW(t.*, 42)` expands the source fields before the final element. Use
`ROW(ROW(t.*), 42)` to retain those fields as a nested row. Existing compatible
named-row casts and record-accepting functions can consume row constructors.
This support does not establish multi-column comparison operator support.

### ARRAY list constructors

`ARRAY[...]` retains its ordered elements and selects a common type using
PostgreSQL's type categories and implicit conversions. For example,
`ARRAY[1,2.5]` has type `numeric[]`; all-unknown strings or NULLs produce
`text[]`. When the element type is wholly unresolved, the result domain remains
open and `ParameterConstraints::allows` checks each supplied input assignment;
for example, `ARRAY[$1,NULL]` admits integer and text inputs with their
corresponding array types. SQL NULL and string literals that have no parameter
source still resolve to `text`. The constructed array is non-null even when its
elements may be NULL. Its implicit output name is `array`; an explicit alias
supplies the output name.

An empty constructor requires an explicit array cast, such as
`ARRAY[]::integer[]`. The cast also supplies the element type to nested
constructors, including nested empty arrays. Nested arrays have the same SQL
type identity as a one-dimensional array of the same element type. Analysis
reports incompatible types and missing type-category facts as diagnostics.

Arrays of anonymous records are supported. PostgreSQL subscripting yields a
generic `record` without accessible field names; use a named composite element
type when subsequent SQL needs field references.

Numbered parameters participate in constructor analysis. The result retains
admissible input and output types and their relationships; explicit casts
constrain the target type. `ARRAY(SELECT ...)` also analyzes its child query, as
described below.

### ARRAY query results

`ARRAY(SELECT ...)` requires exactly one output column and constructs a non-null
array, including for an empty query result or nullable scalar elements. For
example, `ARRAY(SELECT 1 WHERE false)` has type `integer[]`. Unknown literal
outputs resolve to `text`; unresolved parameters keep their input/result
domains. Parameters and correlated references in the child query participate in
analysis. A pending composite star expansion defers the width check until its
fields are known. Array-valued source columns retain the same PostgreSQL array
type identity; analysis does not reject their static types merely because
runtime accumulation has additional shape restrictions. Analysis does not
execute the child query or validate runtime dimensions.

Array subscripting and slices retain their source and index constraints. A known
non-array operand and a known invalid index type can produce distinct
diagnostics. Conflicting requirements on one unresolved parameter instead
produce parameter constraint diagnostics; analysis does not arbitrarily blame
one occurrence. PostgreSQL `int2vector` and `oidvector` can be subscripted using
their element metadata while keeping their distinct nominal identities.

## Dialect and engine-specific behavior

SQLite uses different rules: `(value)` is scalar grouping and multi-element
parentheses are row values. A row value cannot be projected as an ordinary
scalar result or supplied to a scalar input such as `CAST((1, 2) AS TEXT)` or
`COALESCE((1, 2), 3)`. Legal row membership tests and tuple UPDATE assignments
keep their existing behavior.

The ARRAY constructor rules above describe PostgreSQL. SQLite row values have
the scalar-consumer restrictions above; recognizing array-related Turso syntax
does not establish stock SQLite execution support. Array and row parameters use
the shared [parameter rules](parameters.md). Field access follows
[names and column references](names.md#composite-fields).

## Diagnostics

- [array_empty]: an empty constructor has no explicit element type.
- [array_incompatible_types]: constructor operands have no compatible common
  type.
- [array_missing_type_facts]: required constructor type-category facts are
  unavailable.
- [array_subquery_columns]: an ARRAY query has other than one output column.
- [require_array]: a known source does not support array access.
- [array_index_type_mismatch]: a known subscript type cannot be used as an
  index.

[array_empty]: ../../diagnostics/array_empty.md
[array_incompatible_types]: ../../diagnostics/array_incompatible_types.md
[array_missing_type_facts]: ../../diagnostics/array_missing_type_facts.md
[array_subquery_columns]: ../../diagnostics/array_subquery_columns.md
[require_array]: ../../diagnostics/require_array.md
[array_index_type_mismatch]: ../../diagnostics/array_index_type_mismatch.md

See also [parameter constraints](parameters.md#diagnostics) and
[composite field diagnostics](names.md#diagnostics).
