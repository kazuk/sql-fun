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

## Required input and explicit schema levels

```scrut
$ sql-fun test query.sql 2>&1 | head -1
error: the following required arguments were not provided:
```

```scrut
$ sql-fun analyze query.sql --schema-level invalid 2>&1 | head -1
error: invalid value 'invalid' for '--schema-level <SCHEMA_LEVEL>'
```

## Invalid selected inputs fail before connecting

```scrut
$ echo '{"queries":[]}' > "$TMPDIR/empty.json" && \
> sql-fun analyze "$TMPDIR/empty.json#$.queries" 2>&1
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
