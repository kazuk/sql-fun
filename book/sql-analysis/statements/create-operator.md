# CREATE OPERATOR

PostgreSQL `CREATE OPERATOR` makes a binary or prefix operator available to
later statements. Specify `RIGHTARG` for a prefix operator and both
`LEFTARG` and `RIGHTARG` for a binary operator. The named implementation
function must already be declared with those exact input types. Both
`FUNCTION` and the historical spelling `PROCEDURE` name the implementation
function. A schema can hold multiple overloads of a token with distinct
operand types.

```sql
CREATE OPERATOR public.%% (
    LEFTARG = integer,
    RIGHTARG = integer,
    FUNCTION = pg_catalog.int4pl
);

SELECT 1 OPERATOR(public.%%) 2;
```

A commutator or negator can refer to a later operator declaration. The
referenced operator becomes callable once its own declaration completes.
Completion preserves existing relationships. Related operators must return
the same type. These checks follow the corrected PostgreSQL rules, including
when analysis uses an older builtin bundle.
An invalid definition reports an
[`invalid_operator_declaration` diagnostic](../../diagnostics/invalid_operator_declaration.md)
and leaves the catalog unchanged.

`RESTRICT` and `JOIN` require a selectivity estimator with the appropriate
signature in the analysis catalog. The PostgreSQL 17.6 and 18.1 bundles omit
builtin estimators such as `contsel` and `contjoinsel`. A declaration that
calls for one of these missing estimators is rejected, including the first
operator declaration in the bundled `pg_trgm` installation script. Follow
[issue #1217](https://github.com/kazuk/sql-fun/issues/1217) for estimator
metadata. The full script also contains a
[CREATE OPERATOR CLASS](create-operator-class.md) declaration for
`gist_trgm_ops`. Follow [issue #1150](https://github.com/kazuk/sql-fun/issues/1150)
for full extension initialization.

For a token defined in multiple schemas, qualify the operator with
`OPERATOR(schema.token)` to select its schema explicitly. Unqualified lookup
currently selects one schema's overloads for the token and operand count.
Cross-schema overload lookup is tracked in
[issue #797](https://github.com/kazuk/sql-fun/issues/797).

PostgreSQL permits set-returning implementation functions. sql-fun reports
those operator declarations as unsupported and does not register them.
Follow [issue #1218](https://github.com/kazuk/sql-fun/issues/1218).

## ALTER OPERATOR SET

`ALTER OPERATOR … SET (...)` updates the existing overload with the exact
operand types. For a prefix operator, specify `NONE` for the left operand.
An unqualified target uses the first matching signature on the search path.

`RESTRICT` and `JOIN` set selectivity estimators by name. Use `NONE` to clear
an estimator. Omitted settings retain their previous values. Estimator
functions must be available in the analysis catalog, with the same signature
requirements as for `CREATE OPERATOR`. The missing builtin estimator
metadata tracked in [issue #1217](https://github.com/kazuk/sql-fun/issues/1217)
also affects ALTER declarations in extension scripts.

PostgreSQL 17 and 18 also allow adding a `COMMUTATOR` or `NEGATOR` and enabling
`HASHES` or `MERGES`. Linking two operators updates the reciprocal
relationship. Repeating an existing setting is allowed. Replacing an
existing link or disabling an enabled flag reports
[`invalid_operator_declaration`](../../diagnostics/invalid_operator_declaration.md)
and leaves the catalog unchanged. Builtin operators require a catalog bundle
that includes the initial planner settings to validate changes to these four
attributes.

`DROP OPERATOR`, which occurs in the bundled `intarray` update script, still
fails during statement conversion. Follow
[issue #1229](https://github.com/kazuk/sql-fun/issues/1229).
