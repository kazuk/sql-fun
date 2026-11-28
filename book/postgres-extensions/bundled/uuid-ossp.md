# uuid-ossp

Functions for generating UUIDs and supplying namespace constants.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.1 | Initializes. Example resolves. |
| 19 snapshot | 1.1 | Initializes. Example resolves. |

## SQL example

```sql
SELECT uuid_ns_dns();
```

## Analysis boundaries

Analysis resolves the UUID result type. PostgreSQL evaluates namespace
functions and generates UUID values.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/uuid-ossp.html)
