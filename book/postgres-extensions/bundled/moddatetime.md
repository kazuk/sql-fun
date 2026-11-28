# moddatetime

A trigger function that updates a timestamp column on row modification.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example loads from project schema. |
| 18.1 | 1.0 | Initializes. Example loads from project schema. |
| 19 snapshot | 1.0 | Initializes. Example loads from project schema. |

## SQL example

Put this definition in the project schema along with its table.

```sql
CREATE TABLE items (modified timestamp);
CREATE TRIGGER stamp_item BEFORE UPDATE ON items
FOR EACH ROW EXECUTE FUNCTION moddatetime(modified);
```

## Analysis boundaries

Trigger analysis checks the definition and function signature. PostgreSQL
interprets the column-name argument and supplies the timestamp when the
trigger runs.

Passing `CREATE TRIGGER` directly to `analyze` fails during SQL rendering.
Use the project schema to load the trigger definition.
[Tracking issue #1305](https://github.com/kazuk/sql-fun/issues/1305).

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/contrib-spi.html#CONTRIB-SPI-MODDATETIME)
