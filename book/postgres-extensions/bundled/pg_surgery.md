# pg_surgery

Functions for forcing changes to tuple visibility.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves. |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

```sql
SELECT heap_force_kill(NULL::regclass, ARRAY['(0,1)'::tid]);
```

## Analysis boundaries

Analysis resolves the relation and tuple-identifier argument types. It does
not touch tuples, verify their existence, or assess the effects of surgery.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgsurgery.html)
