# Operators and predicates

## Overview

Analysis resolves operand names, selected-catalog operator overloads and
implicit conversions for supported PostgreSQL expressions. Parameters contribute
joint constraints; see [parameter inference](parameters.md). Nullability
describes static knowledge, not execution of an operator or predicate. Custom
catalog functions/operators may leave nullability unknown.

### BETWEEN

Analysis supports `BETWEEN`, `NOT BETWEEN`, and their explicit `ASYMMETRIC`
forms:

```sql
SELECT price BETWEEN $1::numeric AND $2::numeric AS within_range
FROM products
```

Both endpoints are inclusive. Ordinary `BETWEEN` preserves the endpoint order;
it does not reorder reversed bounds. Each bound must support the corresponding
PostgreSQL comparison with the value; compatible mixed types are allowed.
Analysis checks operand references, conversions and parameter constraints and
reports a boolean result. It does not evaluate the predicate's value.

NULL follows SQL's three-valued logic: a NULL operand does not always make the
whole predicate NULL. For example, `0 BETWEEN 1 AND NULL` is false, and
`0 NOT BETWEEN 1 AND NULL` is true. Result nullability is conservative: analysis
can report nullable even for comparisons of non-null operands, and unknown
nullability when comparison information is unresolved. SQLite range predicate
analysis remains unsupported.

### BETWEEN SYMMETRIC

Analysis supports `BETWEEN SYMMETRIC` and `NOT BETWEEN SYMMETRIC`:

```sql
SELECT price BETWEEN SYMMETRIC $1::numeric AND $2::numeric AS within_range
FROM products
```

Either endpoint order is accepted, and both endpoints are inclusive. Operands
must support the corresponding PostgreSQL comparisons; compatible mixed types
are allowed. NULL follows SQL's three-valued comparison logic. Analysis checks
operand references and parameter constraints and describes a boolean result; it
does not evaluate the predicate's value.

### Truth tests

`IS TRUE`, `IS FALSE`, `IS UNKNOWN`, and their `IS NOT` forms produce a non-null
boolean, even when the operand is NULL or a nullable column. `IS UNKNOWN` tests
for a NULL boolean operand. These predicates require a boolean operand or a
legal implicit conversion to boolean. A typed integer or text value needs an
explicit boolean cast; a bare NULL or valid boolean string literal can be
converted directly. Invalid boolean string literals are diagnosed.

Truth predicates constrain parameter inputs to boolean-convertible types.
Reusing a parameter in an incompatible expression produces a
parameter-constraint diagnostic. As with other parameter analysis, admissible
input types describe valid explicitly typed bindings; analysis does not select a
wire-protocol binding.

### Null-safe distinctness

PostgreSQL `IS DISTINCT FROM` and `IS NOT DISTINCT FROM` analyze operand
bindings and equality-compatible parameter types. Builtin comparisons return a
non-null boolean, including comparisons with NULL. A bare NULL does not require
an equality operator or determine a parameter's input type:
`$1 IS DISTINCT FROM NULL` still needs an input binding. A typed NULL, such as
`NULL::integer`, participates in equality type checking. Explicit row pairs
compare corresponding fields and must have the same number of fields. Custom
equality implementations without a non-null result guarantee retain unknown
result nullability. Domain/base-type operator matching is supported, including
distinctness. Named composite compatibility with generic record operators
remains limited
([#804](https://github.com/kazuk/sql-fun/issues/804)). SQLite supports the
equivalent `IS` / `IS NOT` forms and their long spellings without PostgreSQL
equality overload constraints.

### Membership

`IN` and `NOT IN` analyze scalar and row operands with a value list or a
subquery. PostgreSQL resolves comparison operators and implicit conversions
using the selected catalog. A scalar subquery membership requires one output
column; a row operand requires the same number of query output columns.
Incompatible widths and comparison types produce analysis diagnostics.

PostgreSQL predicates return boolean; SQLite predicates return integer. NULL
inputs can produce NULL results. `NOT IN` does not turn a NULL comparison into
true. Result nullability is conservative: analysis does not execute a query to
determine whether its result is empty or whether another list item will match.
Membership participates in the existing parameter analysis; no additional hints
are required by this syntax.

### Pattern matching

PostgreSQL analysis supports `LIKE`, `NOT LIKE`, `ILIKE`, `NOT ILIKE`,
`SIMILAR TO`, and `NOT SIMILAR TO`, including legal `ESCAPE` clauses:

```sql
SELECT name LIKE $1 ESCAPE '!' AS matches FROM products
```

Analysis resolves operand references, operator signatures, conversions and
parameter constraints. PostgreSQL results are boolean. LIKE also supports
compatible `bytea` operands; numeric inputs do not gain an implicit text cast.
NULL inputs, including an explicit NULL escape, can produce NULL. Result
nullability follows the analyzer's conservative catalog-based policy.

SQLite analysis supports `LIKE` and `NOT LIKE`, with an optional `ESCAPE`
expression. Results have integer truth values, and operands follow SQLite's
coercion rules rather than PostgreSQL text constraints. An omitted escape and an
explicit NULL escape remain distinct. Result nullability remains unknown because
applications can override either the two- or three-argument `like()` function,
and analysis does not establish the replacement function's return contract.
SQLite GLOB, MATCH and REGEXP semantic analysis remains unimplemented.

Analysis does not execute patterns or validate runtime pattern contents.

## Dialect and engine-specific behavior

PostgreSQL predicates report boolean results. SQLite membership, distinctness
and LIKE use SQLite integer truth values and coercions. General SQLite
arithmetic, comparison overload/type-affinity and operator-dependent parameter
analysis remain incomplete; the supported predicate forms above do not establish
complete SQLite expression checking. SQLite BETWEEN analysis and
GLOB/MATCH/REGEXP semantic analysis remain unimplemented. Parsing valid SQL is
distinct from analysis support.

### Remaining comparison and operator limitations

SQLite's table-name shorthand `value IN table_name` and `NOT IN table_name`
remain unimplemented and can terminate analysis
([#496](https://github.com/kazuk/sql-fun/issues/496)). The supported value-list
and subquery membership forms do not implement that shorthand.

General quantified comparison subqueries (`ANY`/`SOME`/`ALL`) and row-comparison
subqueries do not yet have complete operand and comparison-operator checking
([#571](https://github.com/kazuk/sql-fun/issues/571)). The membership path used
for IN and supported equality-ANY forms is a separate implemented case; do not
infer general quantified-comparison support from it. Multi-column ordered row
comparisons additionally lack the catalog-backed B-tree operator-family
membership and strategy facts required to establish their semantics
([#792](https://github.com/kazuk/sql-fun/issues/792)). Identifying the
applicable operator by its qualified name and declared operand types remains
incomplete ([#797](https://github.com/kazuk/sql-fun/issues/797)); an operator
spelling or boolean result alone does not establish a valid row comparison.

Schema-qualified PostgreSQL operator names such as `OPERATOR(pg_catalog.+)`
retain their schema identity and participate in ordinary operator resolution.
Quoted identifiers preserve case and embedded punctuation. Operator references
with three or more components can be parsed and rendered, but analysis reports
[cross_database_operator_reference](../../diagnostics/cross_database_operator_reference.md);
it does not resolve operators in another database.

## Diagnostics

- [undefined_operator]: the selected catalog does not establish the requested
  operator.
- [binary_operator_overload_ambiguous]: more than one binary signature remains
  applicable.
- [binary_operator_overload_resolution_failed]: established operand types cannot
select a binary signature.
- [unary_operator_overload_ambiguous]: more than one unary signature remains
  applicable.
- [unary_operator_overload_resolution_failed]: established operand types cannot
select a unary signature.
- [truth_test_operand_type]: a known truth-test operand cannot satisfy the
PostgreSQL boolean requirement.
- [truth_test_invalid_literal]: an unknown literal cannot be interpreted as the
required boolean input.
- [distinct_row_arity]: explicit row operands have different widths.
- [distinct_equality_result]: the selected distinctness equality comparison does
not return boolean.
- [membership_width]: membership input and subquery/row widths are incompatible.
- [membership_comparison_result]: a selected membership comparison does not
return the required truth type.

[undefined_operator]: ../../diagnostics/undefined_operator.md
[binary_operator_overload_ambiguous]: ../../diagnostics/binary_operator_overload_ambiguous.md
[binary_operator_overload_resolution_failed]: ../../diagnostics/binary_operator_overload_resolution_failed.md
[unary_operator_overload_ambiguous]: ../../diagnostics/unary_operator_overload_ambiguous.md
[unary_operator_overload_resolution_failed]: ../../diagnostics/unary_operator_overload_resolution_failed.md
[truth_test_operand_type]: ../../diagnostics/truth_test_operand_type.md
[truth_test_invalid_literal]: ../../diagnostics/truth_test_invalid_literal.md
[distinct_row_arity]: ../../diagnostics/distinct_row_arity.md
[distinct_equality_result]: ../../diagnostics/distinct_equality_result.md
[membership_width]: ../../diagnostics/membership_width.md
[membership_comparison_result]: ../../diagnostics/membership_comparison_result.md

BETWEEN and pattern predicates reuse operator and
[cast diagnostics](types-and-casts.md#diagnostics). Contradictory requirements
on an unresolved input use [parameter diagnostics](parameters.md#diagnostics).
