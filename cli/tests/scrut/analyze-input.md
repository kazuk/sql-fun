# Analyze input selection

The real CLI and server receive literal SQL or explicitly selected file inputs.
The fixture and all paths below belong to this suite.

```scrut
$ cat > "$SQL_FUN_API_FIXTURE/input.json" <<'JSON'
> {"scalar":"SELECT 2 AS scalar",
>  "queries":["SELECT 3 AS first; SELECT 4 AS second", "SELECT 5 AS third"]}
> JSON
> echo 'SELECT 6 AS from_file' > "$SQL_FUN_API_FIXTURE/query.sql"
> function analyze_input() {
>   SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" "$SQL_FUN_BIN" --term-color never \
>     --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
>     analyze --schema-level builtin "$@"
> }
> analyze_input 'SELECT 1 AS result' > "$SQL_FUN_API_FIXTURE/inline.out"
> grep -E '^OK <inline SQL>|result.*pg_catalog.int4' \
>   "$SQL_FUN_API_FIXTURE/inline.out" | awk '{$1=$1; print}'
OK <inline SQL>
0 result pg_catalog.int4 not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/query.sql" > "$SQL_FUN_API_FIXTURE/file.out"
> grep -E 'from_file.*pg_catalog.int4' \
>   "$SQL_FUN_API_FIXTURE/file.out" | awk '{$1=$1; print}'
0 from_file pg_catalog.int4 not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/input.json#$.scalar" > "$SQL_FUN_API_FIXTURE/scalar.out"
> grep -E 'scalar.*pg_catalog.int4' \
>   "$SQL_FUN_API_FIXTURE/scalar.out" | awk '{$1=$1; print}'
0 scalar pg_catalog.int4 not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/input.json#$.queries" > "$SQL_FUN_API_FIXTURE/array.out"
> grep -E '^OK |Statement [01]|(first|second|third).*pg_catalog.int4' \
>   "$SQL_FUN_API_FIXTURE/array.out" | \
>   sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g" | awk '{$1=$1; print}'
OK FIXTURE/input.json#$.queries[0]
Statement 0
0 first pg_catalog.int4 not_null
Statement 1
0 second pg_catalog.int4 not_null
OK FIXTURE/input.json#$.queries[1]
0 third pg_catalog.int4 not_null
```

A path-looking positional argument must be analyzed literally, even when that
file exists. A `#` in inline SQL is likewise not a file selector.

```scrut
$ cd "$SQL_FUN_API_FIXTURE"
> analyze_input 'query.sql' > "$SQL_FUN_API_FIXTURE/path.out" 2> "$SQL_FUN_API_FIXTURE/path.err"
[1]
```

```scrut
$ grep -F 'query_parse' "$SQL_FUN_API_FIXTURE/path.out" | awk '{$1=$1; print}'
query_parse
```

SQL diagnostics remain in the report when explicitly duplicated to stderr.

```scrut
$ analyze_input --diagnostics-to-stderr 'query.sql' \
>   > "$SQL_FUN_API_FIXTURE/duplicate.out" \
>   2> "$SQL_FUN_API_FIXTURE/duplicate.err"
[1]
```

```scrut
$ diff -u "$SQL_FUN_API_FIXTURE/path.out" "$SQL_FUN_API_FIXTURE/duplicate.out"
> test ! -s "$SQL_FUN_API_FIXTURE/path.err"
> grep -F query_parse "$SQL_FUN_API_FIXTURE/duplicate.err"
> grep -E '^ERROR|^  SQL|Result columns' \
>   "$SQL_FUN_API_FIXTURE/duplicate.err" || true
query_parse
```

```scrut
$ grep -F 'from_file' "$SQL_FUN_API_FIXTURE/path.out"
[1]
```

```scrut
$ analyze_input 'SELECT 1 AS result -- # literal' > "$SQL_FUN_API_FIXTURE/hash.out"
> grep -E 'result.*pg_catalog.int4' \
>   "$SQL_FUN_API_FIXTURE/hash.out" | awk '{$1=$1; print}'
0 result pg_catalog.int4 not_null
```

Invalid source choices are rejected before input loading or server setup.

```scrut
$ "$SQL_FUN_BIN" analyze > /dev/null 2> "$SQL_FUN_API_FIXTURE/missing.err"
[2]
```

```scrut
$ grep -F 'required arguments' "$SQL_FUN_API_FIXTURE/missing.err"
error: the following required arguments were not provided:
```

```scrut
$ "$SQL_FUN_BIN" analyze 'SELECT 1' --sql-file does-not-exist > /dev/null 2> "$SQL_FUN_API_FIXTURE/conflict.err"
[2]
```

```scrut
$ grep -F 'cannot be used with' "$SQL_FUN_API_FIXTURE/conflict.err"
error: the argument '[SQL]' cannot be used with '--sql-file <FILE[#JSONPath]>'
```

`test` still accepts a selected file and external expectations. The existing
parameter-bindings and named-cases suite cases exercise those paths end to end.
