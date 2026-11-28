# btree_gist

GiST operator classes for scalar types such as integer and text.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.7 | Initializes. Example resolves with a project schema. |
| 18.1 | 1.8 | Initializes. Example resolves with a project schema. |
| 19 snapshot | 1.9 | Initializes. Example resolves with a project schema. |

## SQL example

Use a project schema containing `CREATE TABLE items (id integer);`.

```sql
CREATE INDEX items_gist_idx ON items USING gist (id);
```

## Analysis boundaries

Analysis resolves the operator class for the indexed column. PostgreSQL must
build the index and enforce any exclusion constraints.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/btree-gist.html)
