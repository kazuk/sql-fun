# tablefunc

Set-returning functions for pivoting, generating values, and traversing
hierarchies.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves. |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

```sql
SELECT normal_rand(1, 0.0, 1.0);
```

## Analysis boundaries

Analysis resolves the set-returning function signature. PostgreSQL generates
the values and executes SQL strings passed to functions such as crosstab.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/tablefunc.html)
