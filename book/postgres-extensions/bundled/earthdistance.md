# earthdistance

The `earth` domain, spherical distance functions, and a distance operator for
points.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.2 | Initializes. Example resolves. |
| 17.6 | 1.1 | Explicit selection initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 18.1 | 1.1 | Explicit selection initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT ll_to_earth(0, 0);
```

## Analysis boundaries

Selecting `earthdistance` also loads [cube](cube.md). PostgreSQL calculates
actual distances. Offline analysis currently fails to bind
`earth_distance(ll_to_earth(0, 0), ll_to_earth(0, 1))` to its `earth`
overload. [Tracking issue #1304](https://github.com/kazuk/sql-fun/issues/1304).

The update path of the `cube` prerequisite contains a
[`DO` block](../../diagnostics/opaque_do_body.md). sql-fun skips the
block's body, including its changes to GiST support function dependencies.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/earthdistance.html)
