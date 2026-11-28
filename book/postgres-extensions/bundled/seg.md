# seg

The `seg` interval type, interval operators, and GiST operator class.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.4 | Initializes. Example resolves. |
| 18.1 | 1.4 | Initializes. Example resolves. |
| 19 snapshot | 1.4 | Initializes. Example resolves. |

## SQL example

```sql
SELECT '1 .. 2'::seg && '2 .. 3'::seg;
```

## Analysis boundaries

Analysis resolves the type and overlap operator. PostgreSQL parses real
interval values and evaluates overlap.

The default version's update path contains a
[`DO` block](../../diagnostics/opaque_do_body.md) that adjusts dependencies
for GiST support functions. sql-fun skips the block's body, so these
dependency changes are unavailable to offline analysis.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/seg.html)
