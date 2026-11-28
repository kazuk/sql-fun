# bloom

The bloom index access method and operator classes for integers and text.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves with a project schema. |
| 18.1 | 1.0 | Initializes. Example resolves with a project schema. |
| 19 snapshot | 1.0 | Initializes. Example resolves with a project schema. |

## SQL example

Use a project schema containing `CREATE TABLE items (id integer);`.

```sql
CREATE INDEX items_bloom_idx ON items USING bloom (id);
```

## Analysis boundaries

Analysis can resolve the access method and column operator classes. PostgreSQL
must build and use the index to test runtime reloptions, performance, or
behavior.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/bloom.html)
