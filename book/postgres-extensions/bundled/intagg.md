# intagg

Integer-array aggregation and expansion declarations.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.1 | Initializes. Example resolves. |
| 19 snapshot | 1.1 | Initializes. Example resolves. |

## SQL example

```sql
SELECT int_array_enum(ARRAY[1, 2]);
```

## Analysis boundaries

Analysis resolves the array expansion function. PostgreSQL evaluates the
aggregate and determines the rows returned by expansion.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/intagg.html)
