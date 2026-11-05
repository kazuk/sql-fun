# Identifier styles in diagnostic messages

These checks use the real CLI, server, and fixture-owned small catalog. Only
semantic name fragments should receive the identifier style; SQL snippets keep
their separate token highlighting.

```scrut
$ fixture="$SQL_FUN_API_FIXTURE"
> cat > "$fixture/yellow.toml" <<'TOML'
> identifier = { foreground = "yellow" }
> keyword = { foreground = "blue" }
> TOML
> cat > "$fixture/green.toml" <<'TOML'
> identifier = { foreground = "green" }
> keyword = { foreground = "blue" }
> TOML
> cat > "$fixture/identifier-cases.json" <<'JSON'
> [{
>   "name": "missing-column",
>   "sql": "SELECT missing AS result FROM (SELECT 1 AS id) AS src",
>   "expected": {
>     "diagnostics": ["column_not_found_in_source"],
>     "columns": [{"name":"result","nullability":"unknown"}]
>   }
> }]
> JSON
> function diagnostic_cli() {
>   SQL_FUN_HOME="$fixture/home" "$SQL_FUN_BIN" \
>     --metadata-file "$fixture/Cargo.toml" --datasource products "$@"
> }
> diagnostic_cli --term-color ansi --highlighter-theme "$fixture/yellow.toml" \
>   analyze --schema-level builtin --diagnostics-to-stderr \
>   'SELECT missing AS result FROM (SELECT 1 AS id) AS src' \
>   > "$fixture/identifier-analyze.out" 2> "$fixture/identifier-analyze.err"
[1]
```

The diagnostic prose is uncolored around the name.

```scrut
$ grep -F $'column \e[33mmissing\e[0m not found in query source' \
>   "$fixture/identifier-analyze.out" > /dev/null
> grep -F $'column \e[33mmissing\e[0m not found in query source' \
>   "$fixture/identifier-analyze.err" > /dev/null
```

Changing the identifier theme changes the message in `test`.

```scrut
$ diagnostic_cli --term-color ansi --highlighter-theme "$fixture/green.toml" \
>   test "$fixture/identifier-cases.json" --schema-level builtin \
>   > "$fixture/identifier-test.out" 2> "$fixture/identifier-test.err"
> grep -F $'column \e[32mmissing\e[0m not found in query source' \
>   "$fixture/identifier-test.err" > /dev/null
> grep -F '1 cases: 1 passed, 0 failed or errored' \
>   "$fixture/identifier-test.out"
1 cases: 1 passed, 0 failed or errored
```

Explicitly disabled color and redirected automatic color retain plain text with
no escape sequences.

```scrut
$ diagnostic_cli --term-color never analyze --schema-level builtin \
>   'SELECT missing AS result FROM (SELECT 1 AS id) AS src' \
>   > "$fixture/identifier-never.out" 2> "$fixture/identifier-never.err"
[1]
```

```scrut
$ diagnostic_cli --term-color auto test "$fixture/identifier-cases.json" \
>   --schema-level builtin > "$fixture/identifier-auto.out" \
>   2> "$fixture/identifier-auto.err"
> grep -F 'column missing not found in query source' \
>   "$fixture/identifier-never.out" > /dev/null
> grep -F 'column missing not found in query source' \
>   "$fixture/identifier-auto.err" > /dev/null
> ! grep -F $'\e[' "$fixture/identifier-never.out"
> ! grep -F $'\e[' "$fixture/identifier-auto.err"
```

```scrut
$ diagnostic_cli --term-color ansi --highlighter-theme "$fixture/yellow.toml" \
>   analyze --schema-level builtin 'SELECT public.no_such_function(1)' \
>   > "$fixture/qualified.out" 2> "$fixture/qualified.err"
[1]
```

```scrut
$ grep -F $'function not found: \e[33mpublic.no_such_function\e[0m' \
>   "$fixture/qualified.out" > /dev/null
> grep -F $'\e[34mSELECT' "$fixture/qualified.out" > /dev/null
```

An explicit underline applies to the name in the message and to identifiers in
the SQL snippet, while the keyword keeps its own style.

```scrut
$ cat > "$fixture/underlined.toml" <<'TOML'
> identifier = { foreground = "yellow", underline = true }
> keyword = { foreground = "blue" }
> TOML
> diagnostic_cli --term-color ansi \
>   --highlighter-theme "$fixture/underlined.toml" \
>   analyze --schema-level builtin 'SELECT public.no_such_function(1)' \
>   > "$fixture/underlined.out" 2> "$fixture/underlined.err"
[1]
```

```scrut
$ grep -F $'function not found: \e[33;4mpublic.no_such_function\e[0m' \
>   "$fixture/underlined.out" > /dev/null
> grep -F $'\e[33;4mpublic\e[0m.\e[33;4mno_such_function\e[0m' \
>   "$fixture/underlined.out" > /dev/null
> grep -F $'\e[34mSELECT' "$fixture/underlined.out" > /dev/null
```

Equal column and alias text is styled at both typed positions.

```scrut
$ diagnostic_cli --term-color ansi --highlighter-theme "$fixture/yellow.toml" \
>   analyze --schema-level builtin \
>   'SELECT missing.missing FROM (SELECT 1 AS id) AS missing' \
>   > "$fixture/repeated.out" 2> "$fixture/repeated.err"
[1]
```

```scrut
$ expected=$'column \e[33mmissing\e[0m not found in subquery '
> expected+=$'\e[33mmissing\e[0m'
> grep -F "$expected" "$fixture/repeated.out" > /dev/null
```
