# initialize method

`initialize` authenticates a TCP connection to `sql-fun-server`. Send it as the
first request on every new connection, and wait for the successful response
before sending API requests. Each connection must authenticate independently.

This page describes the current sql-fun protocol. The server does not currently
provide an MCP or LSP initialization handshake or advertise capabilities.

## Connection and framing

The server listens on `127.0.0.1` using a dynamically allocated port. Its JSON
PID file at `/tmp/<user>/sql-fun-server.pid` contains `pid`, `port`, and `token`.
Here, `<user>` is the server's `--user` argument, which can also be supplied
through `USER`. Connect to the recorded port and use that server's token.

Send each request as a JSON object followed by LF (`\n`). Compact JSON is
recommended. Spaces, tabs, and CR (`\r`) between the object and LF are accepted;
CRLF is therefore accepted too. Responses are compact JSON followed by LF.

## Request

Replace the example token with the `token` from the server PID file. The example
is formatted for readability; send it on one line with a trailing LF.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "sql_fun_token": "<token from the server PID file>"
  }
}
```

`sql_fun_token` must be a string matching the server's token. `initialize` is
separate from `initialize_schema_context`: it authenticates the connection and
does not load a schema context.

## Response

Successful authentication returns the request's `id` and the string `"OK"`:

```json
{"jsonrpc":"2.0","id":1,"result":"OK"}
```

<!-- Method pages are generated in the staged manual build. -->
<!-- rumdl-disable MD057 -->
After this response, the connection can send ordinary API requests using their
existing method names and parameter shapes. The current dispatcher supports
`initialize_schema_context`; it does not add a `sql-fun/` prefix or method
version suffix. See the [schema and query analysis workflow](check-analysis-design.md)
and the [method reference](methods/echo.md).
<!-- rumdl-enable MD057 -->

For example, send this request on one line followed by LF:

```json
{"jsonrpc":"2.0","id":2,"method":"echo","params":{"message":"hello"}}
```

The response echoes the parameters:

```json
{"jsonrpc":"2.0","id":2,"result":{"message":"hello"}}
```

The generated method pages show request and result schemas as JSON Schema.
Follow the links below each schema to inspect referenced definitions. Definitions
belong to their individual request or result schema, even when names match.

## Errors

After authentication, an unknown method or invalid parameters produce an
`error` response and the connection remains usable. API execution errors also
include structured `data` when available. The current error codes are all `0`;
they are placeholders and do not distinguish standardized JSON-RPC errors.
Malformed requests and framing failures can instead terminate the connection.

## Rejection and repeated initialization

Before authentication, an unexpected method, an unacceptable request, or a
missing, incorrectly typed, or mismatched token closes the connection without
a JSON-RPC response. This also applies to requests for `rpc.discover` or
`initialize_schema_context` before authentication.

After authentication, `initialize` is handled as an unknown method. The current
response is:

```json
{"jsonrpc":"2.0","id":2,"error":{"code":0,"message":"unknown method : initialize"}}
```

The connection remains authenticated and can continue sending API requests.
The current error code is a placeholder, not a standardized JSON-RPC state
error code.

## Timeouts and size limits

The current server options are:

| Option | Environment variable | Default | Behavior |
| --- | --- | --- | --- |
| `--auth-timeout` | `AUTH_TIMEOUT` | 5 seconds | Limits the initial request read. |
| `--idle-timeout` | `IDLE_TIMEOUT` | 30 seconds | Limits each wait for more input. |
| `--request-length-limit` | `REQUEST_LENGTH_LIMIT` | 2 MiB | Limits requests after authentication. |

The initial authentication request has a fixed limit: its encoded size,
including trailing whitespace and LF, must be less than 1024 bytes. The
configured request length limit applies after the successful initialization
response and is also exclusive: a request reaching the limit is rejected.

An authentication timeout closes the connection without a response. Input
idle timeouts and request length errors also terminate the connection. The
idle timeout remains active after authentication; the authentication timeout
no longer applies. The current CLI accepts zero timeout values; choose positive
values for normal operation.

The authentication timeout currently covers reading the initial request, not
the subsequent validation or response write.
