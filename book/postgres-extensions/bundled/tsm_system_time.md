# tsm_system_time

The `SYSTEM_TIME` table sampling method.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. CLI sampling metadata unavailable. |
| 18.1 | 1.0 | Initializes. CLI sampling metadata unavailable. |
| 19 snapshot | 1.0 | Initializes. CLI sampling metadata unavailable. |

## SQL example

Use a project schema containing `CREATE TABLE items (id integer);`.

```sql
SELECT id FROM items TABLESAMPLE SYSTEM_TIME (1);
```

## Analysis boundaries

Initialization loads the handler declaration. The CLI still reports
[`tablesample_metadata_unavailable`](../../diagnostics/tablesample_metadata_unavailable.md)
for the example, because it lacks sampling-method metadata. It does not sample
rows or measure a time budget.
[Tracking issue #895](https://github.com/kazuk/sql-fun/issues/895).

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/tsm-system-time.html)
