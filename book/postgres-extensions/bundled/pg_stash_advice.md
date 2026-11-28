# pg_stash_advice

Functions for managing saved query advice.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

```sql
SELECT * FROM pg_get_advice_stashes();
```

## Analysis boundaries

Analysis resolves the function and result columns. It does not access saved
advice, start a background worker, or verify planner behavior.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgstashadvice.html)
