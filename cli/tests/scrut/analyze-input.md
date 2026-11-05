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
> grep -E '<inline SQL> \(statement|\[0\] result:' "$SQL_FUN_API_FIXTURE/inline.out"
<inline SQL> (statement 0)
    [0] result: pg_catalog.int4; not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/query.sql" > "$SQL_FUN_API_FIXTURE/file.out"
> grep -F '[0] from_file:' "$SQL_FUN_API_FIXTURE/file.out"
    [0] from_file: pg_catalog.int4; not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/input.json#$.scalar" > "$SQL_FUN_API_FIXTURE/scalar.out"
> grep -F '[0] scalar:' "$SQL_FUN_API_FIXTURE/scalar.out"
    [0] scalar: pg_catalog.int4; not_null
```

```scrut
$ analyze_input --sql-file "$SQL_FUN_API_FIXTURE/input.json#$.queries" > "$SQL_FUN_API_FIXTURE/array.out"
> grep -E '\(statement [01]\)|\[0\] (first|second|third):' \
>   "$SQL_FUN_API_FIXTURE/array.out" | sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
FIXTURE/input.json#$.queries[0] (statement 0)
    [0] first: pg_catalog.int4; not_null
FIXTURE/input.json#$.queries[0] (statement 1)
    [0] second: pg_catalog.int4; not_null
FIXTURE/input.json#$.queries[1] (statement 0)
    [0] third: pg_catalog.int4; not_null
```

A path-looking positional argument must be analyzed literally, even when that
file exists. A `#` in inline SQL is likewise not a file selector.

```scrut
$ cd "$SQL_FUN_API_FIXTURE"
> analyze_input 'query.sql' > "$SQL_FUN_API_FIXTURE/path.out" 2> "$SQL_FUN_API_FIXTURE/path.err"
[1]
```

```scrut
$ grep -F 'query_parse' "$SQL_FUN_API_FIXTURE/path.err"
query_parse
```

```scrut
$ grep -F 'from_file' "$SQL_FUN_API_FIXTURE/path.out"
[1]
```

```scrut
$ analyze_input 'SELECT 1 AS result -- # literal' > "$SQL_FUN_API_FIXTURE/hash.out"
> grep -F '[0] result:' "$SQL_FUN_API_FIXTURE/hash.out"
    [0] result: pg_catalog.int4; not_null
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
