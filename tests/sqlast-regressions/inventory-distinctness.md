# Null-safe distinctness regression boundaries

Issue #499 adds PostgreSQL `IS DISTINCT FROM` and `IS NOT DISTINCT FROM`
analysis. `distinctness.json` and its explicitly selected project schema run
through the shared source and installed-distribution manifest on PostgreSQL
17/18. Cases assert diagnostics and result names/types/nullability for scalar,
cross-type, NULL, array, explicit row, nested and parameter
expressions. Schema setup belongs in `distinctness.sql`.

Rust tests retain operator-name binding, predicate polarity, coerced operands,
serialization, correlated admissible assignments, and the separation between
strictness and a non-null equality result guarantee. The catalog integration
test needs the real cross-type and polymorphic operator inventory. SQLite
feature tests use the existing bundled rusqlite dependency for small oracle
comparisons and assert operand retention and unconstrained affinities.

Development oracle checks used PostgreSQL 15.19, 16.15, 17.6 and 18.1. All four
agree on the basic NULL truth table, integer/bigint comparison, row field NULL
comparison, boolean result type, and rejection of an untyped parameter compared
only with bare NULL. PostgreSQL 18 additionally confirmed:

- Bare NULL permits comparison with `json` without an equality operator;
  `NULL::json` requires the absent `json = json` operator.
- Explicit row arity must match. `ROW(NULL,NULL) IS DISTINCT FROM NULL` is true;
  this tests the row value itself, rather than SQL's field-sensitive row null test.
- Two untyped parameters in PREPARE infer text, but the constraint API describes
  admissible explicitly typed assignments and does not choose that signature.
- A custom enum equality returning NULL makes both predicates nullable. The
  probe created a temporary schema, enum, SQL function and operator inside a
  rolled-back transaction. Custom strictness also cannot prove a non-null result.

No live database is required by the regression suite. Oracle SQL is development
evidence; the analyzer does not evaluate predicate values.

The maintainer excluded shared domain and named-composite matching fixes from
this PR. Their failing probes are tracked in [#799](https://github.com/kazuk/sql-fun/issues/799), rather than changing the
shared matcher or making the new fixtures claim unsupported coverage.
