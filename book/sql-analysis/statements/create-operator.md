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

sql-fun retains the `RESTRICT` and `JOIN` planner function names, including
schema qualification and quoted identifiers. Analysis accepts these options
when the functions are absent from the analysis catalog and skips checking
their PostgreSQL planner signatures. Follow
[issue #1150](https://github.com/kazuk/sql-fun/issues/1150) for bundled
extension initialization.

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

`RESTRICT` and `JOIN` replace the corresponding planner function names.
Use `NONE` to clear a setting. Omitted settings retain their previous values.
As with `CREATE OPERATOR`, sql-fun retains the names without looking them up
in the analysis catalog.

PostgreSQL 17 and 18 also allow adding a `COMMUTATOR` or `NEGATOR` and enabling
`HASHES` or `MERGES`. Linking two operators updates the reciprocal
relationship. Related operators must return the same type. Repeating an
existing setting is allowed. Replacing an
existing link or disabling an enabled flag reports
[`invalid_operator_declaration`](../../diagnostics/invalid_operator_declaration.md)
and leaves the catalog unchanged. Builtin operators require a catalog bundle
that includes the initial planner settings to validate changes to these four
attributes.

Extension scripts containing `DROP OPERATOR` can be parsed, but schema
initialization stops when analysis reaches the drop. Operator removal from the
analysis catalog is tracked in [issue #1150](https://github.com/kazuk/sql-fun/issues/1150).
