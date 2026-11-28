# cube

The `cube` type, coordinate functions, and geometric operators.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.5 | Initializes. Example resolves. |
| 17.6 | 1.2 | Explicit selection initializes. Example resolves. |
| 18.1 | 1.5 | Initializes. Example resolves. |
| 18.1 | 1.2 | Explicit selection initializes. Example resolves. |
| 19 snapshot | 1.5 | Initializes. Example resolves. |

## SQL example

```sql
SELECT cube_dim('(1,2)'::cube);
```

## Analysis boundaries

Analysis resolves the cube type and function. PostgreSQL parses actual cube
values and evaluates geometric operations.

The default version's update path contains a
[`DO` block](../../diagnostics/opaque_do_body.md) that adjusts dependencies
for GiST support functions. sql-fun skips the block's body, so these
dependency changes are unavailable to offline analysis.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/cube.html)
