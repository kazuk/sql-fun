# pgrowlocks

A set-returning function exposing row locks for a relation.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT locked_row FROM pgrowlocks('items');
```

## Analysis boundaries

Analysis resolves the result columns. It does not read lock state or acquire
locks on the named relation.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgrowlocks.html)
