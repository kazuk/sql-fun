# pg_trgm

Trigram similarity functions, operators, and index operator classes.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.6 | Initializes. Example resolves. |
| 18.1 | 1.6 | Initializes. Example resolves. |
| 19 snapshot | 1.6 | Initializes. Example resolves. |

## SQL example

```sql
SELECT similarity('word', 'words');
```

## Analysis boundaries

Analysis resolves the similarity function. PostgreSQL evaluates trigrams and
similarity thresholds and chooses whether to use an index.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgtrgm.html)
