# ltree

The `ltree`, `lquery`, and `ltxtquery` types, with path and pattern operators.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.3 | Initializes. Example resolves. |
| 18.1 | 1.3 | Initializes. Example resolves. |
| 19 snapshot | 1.3 | Initializes. Example resolves. |

## SQL example

```sql
SELECT 'Top.Sub'::ltree ~ 'Top.*'::lquery;
```

## Analysis boundaries

Analysis resolves the path/pattern operator. PostgreSQL parses actual path and
pattern values and evaluates matches.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/ltree.html)
