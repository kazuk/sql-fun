# Set-operation output contracts

Issue #515 resolves the output of each binary UNION, INTERSECT and EXCEPT
against both operands. The original children retain their own output types;
the operation retains combined output domains and positional implicit coercions.

[set-operations.json](set-operations.json) contains 62 cases per PostgreSQL 17/18
catalog, selected with [set-operations.sql](set-operations.sql) through the shared
manifest. Both source-tree and installed-distribution verification run these
contracts through the CLI, client and analysis server.

The cases cover both operand orders for numeric widening, DISTINCT and ALL,
operator-specific NULL guarantees, incompatible types, unequal widths,
left-side names, unknown strings/NULL, pairwise nested resolution, derived tables,
CTEs and explicit CTE aliases, domains, arrays, ORDER BY, explicit parameter
types and preservation of independent valid output columns.

Assignment cases validate the combined output of parameter-free sets for INSERT
and UPDATE row subqueries. All three operators reject boolean-to-integer
assignments; numeric common outputs accept assignment casts, while all-unknown
outputs resolve to text before destination validation. These cases reproduce
the review finding on PR #913 and run on both maintained regression paths.

Small PostgreSQL 17.6/18.1 development probes establish numeric widening,
integer/boolean and width rejection, pairwise unknown handling, domain
preservation/reduction, array widening and CTE consumption. These database
servers are not test dependencies. Static nullability checks establish sound
non-null guarantees rather than predicting whether a query produces rows.

Focused Rust assertions retain original children, common-type domains and
coercions, serialization, correlated parameter assignments and output domains
through CTE/derived bindings. Minimal explicit catalogs avoid duplicated builtin
setup. Catalog tests inspect rejected view replacement, rejected materialized
view creation and subsequent successful application. Feature-enabled SQLite
coverage compares a legal heterogeneous compound query with its embedded oracle
and verifies that PostgreSQL common-type coercions are not imposed.

Multi-row VALUES output reconciliation remains tracked independently in #565.
`SemanticValues::new` still exposes its first row's metadata, so a set operand
such as `VALUES (1), (1.5)` inherits that existing source limitation. This change
combines the operand outputs supplied by the current analyzers; it does not
implement the separate VALUES boundary or claim that limitation is repaired.
