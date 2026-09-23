# JSON-RPC Error Code Policy for sql-fun

This document defines the structured error code policy used in
`sql-fun-server`'s JSON-RPC interface.

## Error Code Format

All error codes follow the format:

```text
-320XY
```

- `X` (tens place): Represents the **error category**
- `Y` (ones place):
  - `0`: General-purpose (catch-all) error for that category
  - `1-9`: Specific errors for detailed control where necessary

This pattern allows structured interpretation while minimizing boilerplate.

## Error Categories (X)

| X  | Category                      | Meaning                                                                 |
|----|-------------------------------|-------------------------------------------------------------------------|
| 0  | Proxy ↔ Server Errors         | TCP connection, PID file, or unreachable server                         |
| 1  | Unknown / Unexpected Errors   | Panics, logic bugs, or internal unknown states                         |
| 2  | Expected Physical Errors      | Schema I/O failures, pg_query errors, file permissions                 |
| 3  | Client Request Errors         | Missing parameters, wrong types, or structurally invalid logic         |
| 4  | Security / Policy Violations  | MCP not enabled, access to restricted schema/table                     |
| 5  | Not Found                     | Target table, column, schema, or view does not exist                   |
| 6  | (Reserved)                    | Currently unused, reserved for future categories                       |

## Design Principles

- `Y=0` errors use `data.target` and `data.name` to describe context.
- Specific codes (`Y=1..9`) are assigned when client/UI logic benefits from
  clearer discrimination.
- All codes fall within the `-32000` to `-32099` range reserved by JSON-RPC for
  implementation-defined errors.

## Examples

| Code     | Category  | Meaning                                 | Example use case                            |
|----------|-----------|-----------------------------------------|---------------------------------------------|
| -32000   | Proxy     | Server not reachable                    | TCP connection refused                      |
| -32010   | Unexpected| Internal server panic                   | Unwrap of None                              |
| -32020   | IO        | Schema file I/O failure                 | File not found or permission denied         |
| -32021   | IO        | Invalid SQL in schema file              | pg_query parse failure                      |
| -32030   | Client    | Missing parameter                       | `params.sql` not provided                   |
| -32040   | Security  | MCP not allowed                         | `--allow-context mcp` not set               |
| -32050   | Not Found | Generic "not found"                     | `data: { target: "table", name: "user" }` |
| -32051   | Not Found | Table not found                         | `get_columns("user")`                      |
| -32052   | Not Found | Column not found                        | `SELECT email FROM users` with no `email`   |
| -32053   | Not Found | Schema not found                        | `select_schema("preview")`                |

This structure offers a consistent and maintainable foundation for future
JSON-RPC error expansion in `sql-fun`.
