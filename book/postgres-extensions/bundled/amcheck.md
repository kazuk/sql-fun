# amcheck

Functions for checking B-tree indexes and heaps. Version `1.5`, bundled
with PostgreSQL 18.1 and the 19 snapshot, also supplies
`gin_index_check(regclass)`.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.4 | Initializes. Example resolves. |
| 18.1 | 1.5 | Initializes. Example resolves. |
| 19 snapshot | 1.5 | Initializes. Example resolves. |

## SQL example

```sql
SELECT bt_index_check(NULL::regclass);
```

## Analysis boundaries

Analysis resolves the check functions. PostgreSQL must perform index and heap
checks to determine whether a relation is consistent.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/amcheck.html)
