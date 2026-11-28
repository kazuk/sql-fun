# pg_prewarm

A function for preloading relation blocks and functions for managing automatic
prewarming.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT pg_prewarm(NULL::regclass);
```

## Analysis boundaries

Analysis resolves the declared signature. It does not load blocks into a cache
or exercise automatic prewarming.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgprewarm.html)
