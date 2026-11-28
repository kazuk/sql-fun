# lo

The `lo` domain and a trigger function for managing large-object references.
Version `1.2`, bundled with PostgreSQL 18.1 and the 19 snapshot, also supplies
`lo_oid(lo)`.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT NULL::lo AS large_object;
```

## Analysis boundaries

Analysis resolves the domain. It does not check whether a large object exists
or test the reference-management trigger's effects.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/lo.html)
