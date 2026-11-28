# pgcrypto

Hashing, encryption, and random-data functions. Version `1.4`, bundled
with PostgreSQL 18.1 and the 19 snapshot, also supplies `fips_mode()`.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.3 | Initializes. Example resolves. |
| 18.1 | 1.4 | Initializes. Example resolves. |
| 19 snapshot | 1.4 | Initializes. Example resolves. |

## SQL example

```sql
SELECT digest('example', 'sha256');
```

## Analysis boundaries

Analysis resolves the hash function signature. PostgreSQL evaluates the
algorithm and determines whether the selected runtime library supports it.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/pgcrypto.html)
