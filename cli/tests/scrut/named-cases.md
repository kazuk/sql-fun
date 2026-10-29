# Named SQL test cases

The real CLI/client/server path checks both named and legacy inputs. The suite's
small catalog suffices for literals, result columns, and unresolved input bindings.

## Direct named cases and JUnit identities

```scrut
$ cat > "$SQL_FUN_API_FIXTURE/named.json" <<'JSON'
> [
>   {
>     "name": "literal & <value> \"quoted\"",
>     "sql": "SELECT 1 AS value",
>     "expected": {
>       "diagnostics": [],
>       "columns": [{"name":"value","type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"}]
>     }
>   },
>   {
>     "name": "expected-binding-error",
>     "sql": "SELECT 1 AS value WHERE ${id} IS NULL",
>     "expected": {
>       "diagnostics": ["parameter_type_unresolved"],
>       "columns": [{"name":"value","type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"}]
>     }
>   }
> ]
> JSON
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> test "$SQL_FUN_API_FIXTURE/named.json" --schema-level builtin \
> --junit-xml "$SQL_FUN_API_FIXTURE/named.xml" \
> >"$SQL_FUN_API_FIXTURE/named.stdout" 2>"$SQL_FUN_API_FIXTURE/named.stderr"
```

```scrut
$ sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g" "$SQL_FUN_API_FIXTURE/named.stdout"
PASS FIXTURE/named.json::literal & <value> "quoted"
PASS FIXTURE/named.json::expected-binding-error
2 cases: 2 passed, 0 failed or errored
```

```scrut
$ grep -F 'named.json[1]::expected-binding-error (statement 0)' \
> "$SQL_FUN_API_FIXTURE/named.stderr" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
FIXTURE/named.json[1]::expected-binding-error (statement 0)
```

```scrut
$ grep -o 'name="[^"]*"' "$SQL_FUN_API_FIXTURE/named.xml" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
name="sql-fun test"
name="sql-fun"
name="FIXTURE/named.json::literal &amp; &lt;value&gt; &quot;quoted&quot;"
name="sql-fun"
name="FIXTURE/named.json::expected-binding-error"
```

## A later independent named case still passes

```scrut
$ jq '.[0].expected.columns[0].name = "wrong"' \
> "$SQL_FUN_API_FIXTURE/named.json" \
> > "$SQL_FUN_API_FIXTURE/mixed.json"
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> test "$SQL_FUN_API_FIXTURE/mixed.json" --schema-level builtin \
> --junit-xml "$SQL_FUN_API_FIXTURE/mixed.xml" \
> >"$SQL_FUN_API_FIXTURE/mixed.stdout" 2>"$SQL_FUN_API_FIXTURE/mixed.stderr"
[1]
```

```scrut
$ grep -E '^FAIL |^PASS |^[0-9]+ cases:' "$SQL_FUN_API_FIXTURE/mixed.stdout" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
FAIL FIXTURE/mixed.json::literal & <value> "quoted"
PASS FIXTURE/mixed.json::expected-binding-error
2 cases: 1 passed, 1 failed or errored
```

```scrut
$ grep -o 'tests="2" failures="1" errors="0"' "$SQL_FUN_API_FIXTURE/mixed.xml"
tests="2" failures="1" errors="0"
```

## Named selections and legacy SQL selections share expectation semantics

```scrut
$ jq '{cases:.,sql:map(.sql),expected:map(.expected)}' \
> "$SQL_FUN_API_FIXTURE/named.json" \
> > "$SQL_FUN_API_FIXTURE/wrapped.json"
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> test "$SQL_FUN_API_FIXTURE/wrapped.json#$.cases" --schema-level builtin \
> >"$SQL_FUN_API_FIXTURE/selected.stdout" 2>"$SQL_FUN_API_FIXTURE/selected.stderr"
```

```scrut
$ sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g" "$SQL_FUN_API_FIXTURE/selected.stdout"
PASS FIXTURE/wrapped.json#$.cases::literal & <value> "quoted"
PASS FIXTURE/wrapped.json#$.cases::expected-binding-error
2 cases: 2 passed, 0 failed or errored
```

```scrut
$ SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> test "$SQL_FUN_API_FIXTURE/wrapped.json#$.sql" \
> --expect "$SQL_FUN_API_FIXTURE/wrapped.json#$.expected" \
> --schema-level builtin \
> --junit-xml "$SQL_FUN_API_FIXTURE/legacy.xml" \
> >"$SQL_FUN_API_FIXTURE/legacy.stdout" 2>"$SQL_FUN_API_FIXTURE/legacy.stderr"
```

```scrut
$ sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g" "$SQL_FUN_API_FIXTURE/legacy.stdout"
PASS FIXTURE/wrapped.json#$.sql[0]
PASS FIXTURE/wrapped.json#$.sql[1]
2 cases: 2 passed, 0 failed or errored
```

```scrut
$ grep -o 'name="[^"]*"' "$SQL_FUN_API_FIXTURE/legacy.xml" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
name="sql-fun test"
name="sql-fun"
name="FIXTURE/wrapped.json#$.sql[0]"
name="sql-fun"
name="FIXTURE/wrapped.json#$.sql[1]"
```

## Setup failures retain every named case

```scrut
$ SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource missing \
> test "$SQL_FUN_API_FIXTURE/named.json" --schema-level builtin \
> --junit-xml "$SQL_FUN_API_FIXTURE/setup-errors.xml" \
> >"$SQL_FUN_API_FIXTURE/setup-errors.stdout" 2>"$SQL_FUN_API_FIXTURE/setup-errors.stderr"
[1]
```

```scrut
$ grep -E '^ERROR |^[0-9]+ cases:' \
> "$SQL_FUN_API_FIXTURE/setup-errors.stdout" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
ERROR FIXTURE/named.json::literal & <value> "quoted"
ERROR FIXTURE/named.json::expected-binding-error
2 cases: 0 passed, 2 failed or errored
```

```scrut
$ grep -o 'tests="2" failures="0" errors="2"' "$SQL_FUN_API_FIXTURE/setup-errors.xml"
tests="2" failures="0" errors="2"
```

```scrut
$ grep -c '<error>' "$SQL_FUN_API_FIXTURE/setup-errors.xml"
2
```
