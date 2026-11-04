# Expression and query resolution

This note is for maintainers extending SQL analysis. It describes the private
expression/query boundary introduced for issue #288, not a public AST API or a
claim that every retained SQL clause has semantic support.

## Boundary and ownership

`sem/unresolved` owns the recursive syntax component. PostgreSQL and SQLite
conversion receive only syntax wrappers and source metadata. They do not look up
tables, functions, operators, or types. Unresolved nodes own SQL names and values;
they contain neither parser nodes nor parser enum values. SQL/JSON and XML nodes
are typed supporting syntax, not an opaque fallback for unknown parser nodes.

The recursion is closed through expressions, subqueries, query targets and
clauses, FROM items, function/window modifiers, and CTE bodies. INSERT, UPDATE,
DELETE, and MERGE inside CTEs retain their expressions and queries in the same
representation. This does not migrate top-level DDL or statement classification.

`sem/resolution::Catalog` exposes only immutable lookups. `Resolution` borrows
that catalog and owns an ordered, analysis-local diagnostic buffer. It cannot
extend the catalog, alter its search path, register schema objects, or change
source state. Local CTE/FROM/outer scopes are values, not catalog writes.

Resolution returns a result (including a possible fatal error) and diagnostics.
Public adapters replay diagnostics through the original owned parse context,
then return the original public result shape. A reporting error stops replay and
takes precedence over a later resolution error. Function-name spans are captured
during conversion; replay does not recompute them. CASE and COALESCE collect
diagnostics between child expressions, preserving their traversal order.

## Syntax inventory

The input inventory is based on the locked `pg_query` 6.2.0 raw parser and
`turso_parser` 0.7.2, rather than their post-analysis/planner node inventories.

| Family | Retained common syntax | Semantic baseline |
| --- | --- | --- |
| Values and names | Literal values, qualified names, stars, parameter identity, type modifiers and array bounds | Existing literal, column, and cast analysis; parameters and star expansion remain limited |
| Expressions | Operators and their kinds, boolean/null/truth tests, rows/lists/arrays, CASE, casts, collation, indirection, named arguments | Supported PostgreSQL operations share existing type/nullability helpers |
| Subqueries | Scalar, EXISTS, ANY/ALL, row/array sublinks, test expression and operator | Legacy composite-valued sublink representation is unchanged |
| Calls | Arguments, star/distinct/variadic, aggregate order, WITHIN GROUP, FILTER, OVER | PostgreSQL overload/cast resolution; modifiers retained even when not analyzed |
| Query body | Targets and aliases, DISTINCT ON, grouping sets, HAVING, named windows, VALUES, nested sets and ALL | Targets, WHERE, FROM, and set result columns; other clauses retain the existing limitations |
| Query tail | Sort direction/null order/USING, LIMIT/OFFSET/FETCH WITH TIES, locking | Retained independently of the limited public semantic SELECT shape |
| FROM | Relations, aliases/column aliases, LATERAL, joins and constraints, ROWS FROM, ordinality, typed columns, TABLESAMPLE | Existing table/view/function/join binding and nullability |
| WITH | Ordered entries, aliases, recursion/materialization, SEARCH/CYCLE | Sequential local CTE scopes; recursive self-binding is not implemented |
| Data-modifying CTE | DML targets, assignments, source/from/using, predicates, RETURNING, conflict inference/action, MERGE actions | INSERT/UPDATE/DELETE keep their existing empty semantic statements; MERGE is not analyzed |
| Windows | Inheritance, partition/order, ROWS/RANGE/GROUPS, bounds, exclusion | Syntax preservation only |
| SQL/JSON | Constructors, aggregates, query functions, passing/output/format/behavior clauses, JSON_TABLE and nested columns | Syntax preservation only |
| XML | XML expressions, attributes, serialization, XMLTABLE namespaces and column clauses | No new XML semantic support |
| SQLite-specific | Parenthesized FROM, index hints, IN-table calls, RAISE, implicit result names, CTE column modifiers, Turso expression extensions | Preserved without converting SQLite syntax into PostgreSQL parser nodes |

Post-analysis fields such as type OIDs, resolved column indexes, planner
registers, and coerced JSON expressions are not raw syntax. SQLite planner-only
Column/RowId/Register/SubqueryResult nodes are rejected as boundary misuse.
Unknown raw variants must be implemented explicitly rather than stored as parser
payloads or silently discarded.

## Compatibility and dialect decisions

The PostgreSQL baseline includes some incomplete behavior: VALUES currently
produces no inferred columns; qualified stars do not expand projections; set
result columns come from the left branch; query ordering/grouping/window clauses
are not validated by the semantic SELECT representation. Retaining these clauses
in common syntax does not change those behaviors. Existing unimplemented semantic
families remain explicitly unimplemented. Raw PostgreSQL families previously
rejected with an analysis error still return an error; representing their syntax
does not introduce semantic support or replace that rejection with a panic.

CTE column aliases and visibility in derived queries now use local scopes.
Nested scalar subqueries can see outer bindings, and LATERAL derived tables can
see preceding FROM items. Non-LATERAL derived tables do not receive sibling FROM
bindings. These scope corrections have independent result-metadata tests.

Nested set-operation branches now retain their recursive semantic shape instead
of being analyzed as empty simple SELECT queries. Qualified operators retain their
schema for lookup; the old expression adapter rejected two-part operator names.
Both corrections have focused result-metadata regression cases. Function-name
spans also retain comments between qualified name components.

SQLite SELECT now reaches the shared resolver, including derived queries,
nonrecursive CTEs, joins, compounds, and same-typed CASE branches. SQLite implicit
result names are retained from its frontend; portable examples should use AS.
SQLite CASE does not insert PostgreSQL implicit casts between different result
types. Unary numeric literals, boolean combinations, null tests, BETWEEN, and
collation retain their existing SQLite handling. SQLite arithmetic/comparison
operators and function affinity/overload semantics remain unimplemented instead
of consulting the PostgreSQL operator/function catalog. Case-insensitive name
binding, projection expansion, parameters, recursive CTEs, and full compound type
unification are not
newly claimed as supported.

## Verification locations

- `sem/unresolved/tests.rs`: context-free syntax inventory fixtures, recursive
  queries, dialect comparisons, and the public SQLite SELECT entry point.
- `sem/unresolved/tests/test_helpers.rs`: a fixture that implements only Catalog,
  proving the resolver needs no catalog-write interface.
- `sem/resolution.rs`: diagnostic ordering, fatal-result preservation, and
  reporting-error precedence tests.
- `tests/query_resolution.rs`: public PostgreSQL rejection behavior in the normal
  library build, separate from unit-test-only internal-invariant panics.
- Existing expression/type-system tests: PostgreSQL regression coverage through
  the public adapters. The former AdventureWorks schema fixture also provided
  coverage during this work; that test and its example schema have since been
  removed.

When adding syntax, update both applicable converters and the inventory fixture.
When adding semantic support, keep recursion in common nodes and add explicit
dialect tests. Do not reintroduce parser-specific traversal into resolution.
