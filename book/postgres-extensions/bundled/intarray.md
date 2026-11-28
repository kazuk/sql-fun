# intarray

Integer-array operators, functions, and index operator classes.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.5 | Initializes. Example resolves. |
| 18.1 | 1.5 | Initializes. Example resolves. |
| 19 snapshot | 1.5 | Initializes. Example resolves. |

## SQL example

```sql
SELECT idx(ARRAY[1, 2], 2);
```

## Analysis boundaries

Analysis resolves array signatures and operators. PostgreSQL evaluates array
operations, including the extension's runtime restrictions on null elements.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/intarray.html)
