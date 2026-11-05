# Unresolved application input bindings

The test-owned small catalog is sufficient for input hints and NULL predicates.
Real PostgreSQL catalog and result-cast coverage lives in the SQLAST suite.

An unresolved input is an error even when the result column is known. A later
independent input still completes. An explicit input hint resolves the binding.

```scrut
$ cat > "$SQL_FUN_API_FIXTURE/bindings.json" <<'JSON'
> {
>   "queries": [
>     "SELECT 1 AS known WHERE ${id} IS NULL",
>     "SELECT 1 AS known WHERE ${id:int4} IS NULL"
>   ],
>   "expected": [
>     {
>       "diagnostics": ["parameter_type_unresolved"],
>       "columns": [{
>         "name": "known",
>         "type": {
>           "schema_name": "pg_catalog",
>           "local_name": "int4",
>           "is_array": false
>         },
>         "nullability": "not_null"
>       }]
>     },
>     {
>       "diagnostics": [],
>       "columns": [{
>         "name": "known",
>         "type": {
>           "schema_name": "pg_catalog",
>           "local_name": "int4",
>           "is_array": false
>         },
>         "nullability": "not_null"
>       }]
>     }
>   ]
> }
> JSON
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> analyze --sql-file "$SQL_FUN_API_FIXTURE/bindings.json#$.queries" \
> --schema-level builtin >"$SQL_FUN_API_FIXTURE/bindings.stdout" \
> 2>"$SQL_FUN_API_FIXTURE/bindings.stderr"
[1]
```

```scrut
$ grep -E '^ERROR |^OK |^    \$1 .* id |^    0 .*known ' \
>   "$SQL_FUN_API_FIXTURE/bindings.stdout" | \
>   sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g" | awk '{$1=$1; print}'
ERROR FIXTURE/bindings.json#$.queries[0]
$1 id unresolved unknown
0 known pg_catalog.int4 not_null
OK FIXTURE/bindings.json#$.queries[1]
$1 id pg_catalog.int4 unknown
0 known pg_catalog.int4 not_null
```

```scrut
$ grep -Eo 'parameter_type_unresolved|application input binding[^;]+' \
> "$SQL_FUN_API_FIXTURE/bindings.stdout"
parameter_type_unresolved
application input binding for parameter $1 is unresolved
```

Expecting the diagnostic and known column contract passes without asserting a
finalized executable binding for the first input.

```scrut
$ SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> test "$SQL_FUN_API_FIXTURE/bindings.json#$.queries" \
> --expect "$SQL_FUN_API_FIXTURE/bindings.json#$.expected" \
> --schema-level builtin >"$SQL_FUN_API_FIXTURE/bindings-test.stdout" \
> 2>"$SQL_FUN_API_FIXTURE/bindings-test.stderr"
> tail -1 "$SQL_FUN_API_FIXTURE/bindings-test.stdout"
2 cases: 2 passed, 0 failed or errored
```
