# pg_visibility

Functions exposing relation visibility-map information.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

Include `"pg_stat_scan_tables"` in the datasource's `roles` list so the
extension's grants can resolve during initialization.

```sql
SELECT all_visible FROM pg_visibility_map(NULL::regclass, 0);
```

## Analysis boundaries

Analysis resolves the visibility-map result columns. It does not inspect
visibility maps or establish relation visibility.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgvisibility.html)
