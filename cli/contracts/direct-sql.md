# Direct SQL CLI contracts

## Help works without runtime tools

```scrut
$ sql-fun analyze --help | head -1
Analyze SQL without Rust extraction or lockfiles
```

```scrut
$ sql-fun test --help | head -1
Test diagnostic sets and result-column contracts without executing SQL
```

```scrut
$ sql-fun analyze --help | grep -A1 -- '--no-column-access'
      --no-column-access
          Disable relation/column access analysis and its output
```

```scrut
$ sql-fun test --help | grep -- '--no-column-access'
[1]
```

```scrut
$ sql-fun analyze --help | grep -A1 -- '--diagnostics-to-stderr'
      --diagnostics-to-stderr
          Additionally write SQL diagnostics to stderr
```

```scrut
$ sql-fun test --help | grep -- '--diagnostics-to-stderr'
[1]
```

## Required input and explicit schema levels

```scrut
$ sql-fun test 2>&1 | head -1
error: the following required arguments were not provided:
```

```scrut
$ sql-fun analyze query.sql --schema-level invalid 2>&1 | head -1
error: invalid value 'invalid' for '--schema-level <SCHEMA_LEVEL>'
```

## Invalid selected inputs fail before connecting

```scrut
$ echo '{"queries":[]}' > "$TMPDIR/empty.json" && \
> sql-fun analyze --sql-file "$TMPDIR/empty.json#$.queries" 2>&1
Error: SQL input array must not be empty
[1]
```

```scrut
$ echo '{"queries":["SELECT 1"]}' > "$TMPDIR/one.json" && \
> echo '[]' > "$TMPDIR/expected.json" && \
> sql-fun test "$TMPDIR/one.json#$.queries" \
> --expect "$TMPDIR/expected.json" 2>&1
Error: expectation count 0 does not match SQL input count 1
[1]
```

## Named case inputs are validated before connection

These commands run with no runtime tools. Invalid content must fail before a
server is started or schema initialization accesses metadata. No report is
invented for invalid input, including a malformed later case.

```scrut
$ echo '[]' > "$TMPDIR/cases.json" && \
> sql-fun test "$TMPDIR/cases.json" 2>&1
Error: test case array must not be empty
[1]
```

```scrut
$ cat > "$TMPDIR/cases.json" <<'JSON'
> [
>   {"name":"same","sql":"SELECT 1","expected":[]},
>   {"name":"same","sql":"SELECT 2","expected":[]}
> ]
> JSON
> sql-fun test "$TMPDIR/cases.json" > "$TMPDIR/duplicate.log" 2>&1
[1]
```

```scrut
$ sed "s|$TMPDIR|TMP|g" "$TMPDIR/duplicate.log"
Error: duplicate test case name "same" at TMP/cases.json[1]
```

```scrut
$ cat > "$TMPDIR/cases.json" <<'JSON'
> [
>   {"name":"valid","sql":"SELECT 1","expected":[]},
>   {"name":"invalid","sql":"SELECT 2","expected":{}}
> ]
> JSON
> sql-fun test "$TMPDIR/cases.json" --junit-xml "$TMPDIR/invalid.xml" \
> > "$TMPDIR/invalid.log" 2>&1
[1]
```

```scrut
$ head -1 "$TMPDIR/invalid.log" | sed "s|$TMPDIR|TMP|g"
Error: invalid expected contract for TMP/cases.json[1]::invalid
```

```scrut
$ test ! -e "$TMPDIR/invalid.xml"
```

```scrut
$ echo '{"cases":[]}' > "$TMPDIR/cases.json" && \
> sql-fun test "$TMPDIR/cases.json#$.missing" > "$TMPDIR/selector.log" 2>&1
[1]
```

```scrut
$ head -1 "$TMPDIR/selector.log" | sed "s|$TMPDIR|TMP|g"
Error: selector TMP/cases.json#$.missing must match exactly one node
```

An explicit `--expect` keeps the legacy SQL string contract and rejects case
objects instead of silently ignoring the external expectations.

```scrut
$ echo '{"cases":[{"name":"a","sql":"SELECT 1","expected":[]}]}' \
> > "$TMPDIR/cases.json" && \
> sql-fun test "$TMPDIR/cases.json#$.cases" --expect "$TMPDIR/expected.json" 2>&1
Error: SQL array elements must be strings
[1]
```
