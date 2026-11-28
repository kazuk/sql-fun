# unaccent

A text-search dictionary and functions for removing accents.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.1 | Initializes. Example resolves. |
| 19 snapshot | 1.1 | Initializes. Example resolves. |

## SQL example

```sql
SELECT unaccent('Hôtel');
```

## Analysis boundaries

Analysis resolves the function signature. It does not load server rule files
or verify the resulting accent substitutions.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/unaccent.html)
