# ROWS FROM semantic sources (#508)

The `rows-from` project-schema fixture runs 44 cases against the bundled
PostgreSQL 17 and 18 catalogs. The shared manifest selects it for both source
verification and installed-distribution verification. Its project objects
are `rows508_pair`, a two-field named composite, and `rows508_srf`, a small
set-returning function used for named-argument placement checks. PostgreSQL
is not a dependency of these maintained tests.

The observable contracts are ordered result names, types, conservative
nullability, and complete diagnostics. Cases cover SQL value forms, COALESCE,
GREATEST/LEAST, NULLIF, CAST, EXTRACT, ordinary/mixed function lists, named
composites, implicit and positional aliases, qualified/unqualified lookup,
duplicates, missing columns, ordinality, outer joins, lateral references,
parameter-dependent values, and recovery after rejected inputs. Cast cases
retain the names of arrays and scalar subquery outputs. Illegal nested SRFs
and aggregate/window calls retain diagnostics; a subquery keeps its own
aggregate scope. Existing record-definition fixtures remain in
`schema-table-functions.json` (#393).

Small read-only development probes on PostgreSQL 17/18 agreed on:

- CURRENT_DATE, CURRENT_USER, USER, CURRENT_ROLE and scalar COALESCE,
  GREATEST/LEAST, NULLIF, CAST and EXTRACT are legal ROWS FROM elements.
- `ROWS FROM(generate_series(1,2), generate_series(1,1)) WITH ORDINALITY`
  returns `(1,1,1)` and `(2,NULL,2)`: constituents can be null-extended,
  independently of their expression nullability.
- For one scalar element, a source alias also becomes its default column name,
  with or without ROWS FROM and with or without ordinality. Multiple elements
  keep their individual output names. Composite fields keep their field names.
- Bare constants and bare ROW constructors are not legal element syntax.
  CAST of a row to a named composite or generic record is legal.
- Nested SRFs in COALESCE, GREATEST and ordinary function arguments are illegal.
  An identity CAST of a top-level SRF is legal; a CAST that changes its type
  makes it a nested SRF and is illegal. Aggregate calls are illegal at the
  element's own query level. `OVER` in this grammar is a parse error;
  `row_number()` without OVER is diagnosed during semantic analysis.

The unresolved native-input case preserves the existing parameter policy:
an SQL result cast does not necessarily select a unique application input
binding. Its output domain may remain unresolved and must not acquire a
fabricated type. The explicitly hinted scalar/composite cases verify known
output metadata without adding another hint syntax.

Focused Rust tests beside the element implementation retain assertions that
cannot be represented by CLI expectations: source-versus-expression identity,
visible column binding, expression/operand retention after rejection,
serialization (including the existing ordinary-invocation wire shape), legacy
analysis entry points, unselected-operand traversal, and correlated parameter
constraints. They use small explicit catalogs via crate-root helpers, without
loading builtin catalogs per test.

Review regressions distinguish lateral bound columns from nested SRF invocations,
and identity casts from converting casts for named-argument SRFs. Placement uses
an agreed concrete return type only when every retained overload candidate has
that type; differing or polymorphic candidates retain a pending placement outcome
alongside the existing deferred signature, rather than selecting an overload.
Ordinary semantic traversal continues through bound source values for constraint
and parameter consumers; only placement validation treats those bindings as opaque.
