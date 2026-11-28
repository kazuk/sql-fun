# hstore

The `hstore` key/value type, lookup operators, and related functions.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.8 | Initializes. Example resolves. |
| 18.1 | 1.8 | Initializes. Example resolves. |
| 19 snapshot | 1.8 | Initializes. Example resolves. |

## SQL example

```sql
SELECT 'a=>b'::hstore -> 'a'::text;
```

## Analysis boundaries

Analysis resolves the type and lookup operator. PostgreSQL parses hstore input
and evaluates lookups and subscripts.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/hstore.html)
