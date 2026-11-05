# Derived-table alias diagnostics survive transport

The real CLI/client/server path must report an excess derived-column alias as a
SQL error. The shared SQLAST suite covers the same query and a valid alias using
real PostgreSQL builtin catalogs; the small catalog here suffices for literals.

```scrut
$ echo 'SELECT s.second FROM (SELECT 1) AS s(first, second);' > "$SQL_FUN_API_FIXTURE/derived-alias.sql"
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> analyze --sql-file "$SQL_FUN_API_FIXTURE/derived-alias.sql" \
> --schema-level builtin \
> --no-column-access \
> >"$SQL_FUN_API_FIXTURE/derived-alias.stdout" 2>"$SQL_FUN_API_FIXTURE/derived-alias.stderr"
[1]
```

Analysis remains incomplete, but delivers an ordinary error diagnostic instead
of failing the request with a JSON-RPC decoding error.

```scrut
$ grep -F '  Complete: false' "$SQL_FUN_API_FIXTURE/derived-alias.stdout"
  Complete: false
```

```scrut
$ grep -F 'sub_query_not_have_column_for_derived_column_name' "$SQL_FUN_API_FIXTURE/derived-alias.stderr"
sub_query_not_have_column_for_derived_column_name
```

```scrut
$ grep -F '  × subquery has no column for the derived column name' "$SQL_FUN_API_FIXTURE/derived-alias.stderr"
  × subquery has no column for the derived column name
```

```scrut
$ grep -E 'JsonRpcError|expected tuple variant' "$SQL_FUN_API_FIXTURE/derived-alias.stderr"
[1]
```
