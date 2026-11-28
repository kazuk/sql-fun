# file_fdw

A foreign-data wrapper declaration for server-side files.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves. |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

```sql
SELECT file_fdw_handler();
```

## Analysis boundaries

Analysis resolves the wrapper handler. It cannot check server-side file paths,
permissions, file formats, or wrapper option values against a live server.

The CLI rejects standalone `CREATE SERVER` as unsupported.
[Tracking issue #1306](https://github.com/kazuk/sql-fun/issues/1306).

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/file-fdw.html)
