# pageinspect

Functions for examining relation pages and decoding their contents.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.12 | Initializes. Example resolves. |
| 18.1 | 1.13 | Initializes. Example resolves. |
| 19 snapshot | 1.13 | Initializes. Example resolves. |

## SQL example

```sql
SELECT get_raw_page('items', 0);
```

## Analysis boundaries

Analysis resolves function signatures. It does not read relation pages,
inspect their contents, or verify runtime access privileges.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pageinspect.html)
