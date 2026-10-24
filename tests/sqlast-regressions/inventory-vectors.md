# Vector diagnostic migration

`sql-fun-sqlast/tests/vectors.rs::invalid_vector_inputs` was removed after its
four cases passed through the real CLI with PostgreSQL 18 catalogs. The original
case order (reverse, incompatible_cast, scalar, wrong_element) maps directly to
`vectors.json` indices 0–3. All originally required diagnostic codes are
retained.
The scalar table-function failure also produces `query_reference_not_found`:
failed table-function binding provides no source for `*`. The CLI asserts both
codes, rather than the original subset assertion. It also checks static result
columns: rejected casts retain their target type; failed scalar overload has an
unresolved type; the failed table source has no result columns.

PostgreSQL 18 oracle queries confirm rejection of scalar `unnest(smallint)` and
`array_position(int2vector, boolean)` overloads, and rejection of both
`smallint[]::int2vector` and `oidvector::smallint[]` conversions. These are
development checks;
the suite does not depend on a live database.

Other vector tests remain Rust coverage for overload bindings and coerced
arguments, catalog lookup/cast contexts, serialization, or correlated parameter
constraints. Their internal assertions are not superseded by these diagnostics.
