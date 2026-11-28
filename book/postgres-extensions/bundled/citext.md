# citext

The case-insensitive `citext` type, its comparisons, casts, and string
functions.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.6 | Initializes. Example resolves. |
| 18.1 | 1.8 | Initializes. Example resolves. |
| 19 snapshot | 1.8 | Initializes. Example resolves. |

## SQL example

```sql
SELECT 'Example'::citext = 'example'::citext;
```

## Analysis boundaries

Analysis resolves the comparison; actual case folding depends on PostgreSQL's
locale and collation behavior. Initialization can report [unverified
binary-cast
compatibility](../../diagnostics/unverified_binary_cast_compatibility.md) for
this extension.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/citext.html)
