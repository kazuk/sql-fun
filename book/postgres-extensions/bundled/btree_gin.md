# btree_gin

GIN operator classes for scalar types such as integer and text.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.3 | Initializes. Example resolves with a project schema. |
| 18.1 | 1.3 | Initializes. Example resolves with a project schema. |
| 19 snapshot | 1.4 | Initializes. Example resolves with a project schema. |

## SQL example

Use a project schema containing `CREATE TABLE items (id integer);`.

```sql
CREATE INDEX items_gin_idx ON items USING gin (id);
```

## Analysis boundaries

Analysis resolves the operator class for the indexed column. PostgreSQL must
build the index and determine whether a query can use it.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/btree-gin.html)
