# isn

Product identifier types, including `isbn` and `ean13`, and casts between
them.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.3 | Initializes. Example resolves. |
| 19 snapshot | 1.3 | Initializes. Example resolves. |

## SQL example

```sql
SELECT NULL::isbn AS book_number, NULL::ean13 AS barcode;
```

## Analysis boundaries

Analysis resolves the identifier types and records the declared casts. It can
report [unverified binary-cast
compatibility](../../diagnostics/unverified_binary_cast_compatibility.md);
PostgreSQL validates identifier formatting and check digits when evaluating
values.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/isn.html)
