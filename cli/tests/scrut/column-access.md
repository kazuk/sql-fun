# Analyze access defaults and opt-out

These cases use the shared real server and a small test-owned relation. They
verify transported access facts rather than repeating the semantic analyzer's
catalog tests. The same suite runs against source builds and installed binaries.

```scrut
$ echo 'CREATE TABLE events (id int4);' > "$SQL_FUN_API_FIXTURE/access-schema.sql"
> cat > "$SQL_FUN_API_FIXTURE/access.yaml" <<'YAML'
> queries:
>   - |
>     SELECT id FROM events WHERE id IS NULL;
>     UPDATE events SET id = id RETURNING id;
>     DELETE FROM events RETURNING id
>   - SELECT (SELECT id FROM events WHERE id IS NULL) AS nested
>   - SELECT 1 AS literal
>   - SELECT id, ${input} AS argument FROM events
> YAML
> SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> --schema-file "$SQL_FUN_API_FIXTURE/access-schema.sql" \
> analyze "$SQL_FUN_API_FIXTURE/access.yaml#$.queries" \
> >"$SQL_FUN_API_FIXTURE/access.stdout" 2>"$SQL_FUN_API_FIXTURE/access.stderr"
[1]
```

All statements retain their identities. Every purpose for the same column is
shown, including relation-level actions without a column.

```scrut
$ grep -E '^  Column access|^    public.events' "$SQL_FUN_API_FIXTURE/access.stdout"
  Column access (3):
    public.events (Table): ReadRows
    public.events.id (Table): SelectFilter
    public.events.id (Table): SelectOutput
  Column access (5):
    public.events (Table): ReadRows
    public.events (Table): UpdateRows
    public.events.id (Table): UpdateInput
    public.events.id (Table): UpdateReturning
    public.events.id (Table): UpdateTarget
  Column access (3):
    public.events (Table): DeleteRows
    public.events (Table): ReadRows
    public.events.id (Table): DeleteReturning
  Column access (2):
    public.events (Table): ReadRows
    public.events.id (Table): SelectOutput
  Column access (0):
  Column access (2):
    public.events (Table): ReadRows
    public.events.id (Table): SelectOutput
```

The nested WHERE reference retains its outer output purpose. The final input is
incomplete but still reports its resolved accesses and ordinary result columns.

```scrut
$ grep -E 'statement [12]|Complete: false|\$1 input:|\[0\] id:' \
> "$SQL_FUN_API_FIXTURE/access.stdout" | \
> sed "s|$SQL_FUN_API_FIXTURE|FIXTURE|g"
    [0] id: pg_catalog.int4; nullable
FIXTURE/access.yaml#$.queries[0] (statement 1)
    [0] id: pg_catalog.int4; nullable
FIXTURE/access.yaml#$.queries[0] (statement 2)
    [0] id: pg_catalog.int4; nullable
  Complete: false
    $1 input: unresolved; unknown
    [0] id: pg_catalog.int4; nullable
```

Opt-out preserves exit status, diagnostics, and all ordinary output, while
omitting the entire access section. Request-level opt-out and lightweight `test`
requests are additionally checked by the CLI request-capture integration tests.

```scrut
$ SQL_FUN_HOME="$SQL_FUN_API_FIXTURE/home" \
> "$SQL_FUN_BIN" --term-color never \
> --metadata-file "$SQL_FUN_API_FIXTURE/Cargo.toml" --datasource products \
> --schema-file "$SQL_FUN_API_FIXTURE/access-schema.sql" \
> analyze "$SQL_FUN_API_FIXTURE/access.yaml#$.queries" --no-column-access \
> >"$SQL_FUN_API_FIXTURE/access-disabled.stdout" 2>"$SQL_FUN_API_FIXTURE/access-disabled.stderr"
[1]
```

```scrut
$ grep -E 'Column access|\(Table\):' "$SQL_FUN_API_FIXTURE/access-disabled.stdout"
[1]
```

```scrut
$ grep -v -E '^  Column access|^    .* \(Table\):' \
> "$SQL_FUN_API_FIXTURE/access.stdout" \
> > "$SQL_FUN_API_FIXTURE/access-ordinary.stdout"
> diff -u "$SQL_FUN_API_FIXTURE/access-ordinary.stdout" \
> "$SQL_FUN_API_FIXTURE/access-disabled.stdout"
> diff -u "$SQL_FUN_API_FIXTURE/access.stderr" \
> "$SQL_FUN_API_FIXTURE/access-disabled.stderr"
```
