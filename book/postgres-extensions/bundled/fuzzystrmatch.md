# fuzzystrmatch

Functions for phonetic matching and edit distance.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT levenshtein('kitten', 'sitting');
```

## Analysis boundaries

Analysis resolves declared function signatures. PostgreSQL evaluates the
phonetic encodings or edit distances.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/fuzzystrmatch.html)
