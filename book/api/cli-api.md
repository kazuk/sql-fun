# Using the Server API via `sql-fun api`

Stream JSON Lines requests to the authenticated, shared per-user
`sql-fun-server`. Install both `sql-fun-cli` and `sql-fun-server` and make the
server executable available on `PATH`, or set `SQL_FUN_SERVER` to its path.
The [method reference](methods.md) describes the server's declared request and
result schemas. For a direct connection without the CLI bridge, see
[connection and authentication](initialize.md).

```sh
printf '%s\n' '{"method":"echo","params":{"hello":[1,null,true]}}' |
  sql-fun api --user alice
```

The diagnostic `echo` method returns its object or array parameters unchanged.
It needs no datasource or schema setup. For example, stdout contains a response
like this (the client assigns the ID):

```json
{"jsonrpc":"2.0","id":1,"result":{"hello":[1,null,true]}}
```

For command syntax and options, see the [`sql-fun api` CLI reference](../cli/api.md).

## Connection and launch choices

`--user` selects the server namespace and overrides `USER`. Supply either;
a missing or invalid user is an error. The server is shared by commands using
the same user. A successful existing connection is reused without launching
another server. After the last client disconnects, the shared server retains its
cache for 180 seconds by default, then shuts down. A connection during that period
reuses the server and cancels the pending shutdown. A later command can launch a
new server after normal idle shutdown removes its registration.

To change the idle period for a newly launched server, set
`SQL_FUN_SERVER_IDLE_TIMEOUT` to a number of seconds; `0` disables automatic idle
shutdown. For example:

```sh
SQL_FUN_SERVER_IDLE_TIMEOUT=600 sql-fun api --user alice
```

This setting is read when the server starts; it does not reconfigure an already
running shared server. An open client connection prevents server idle shutdown
even when it is not sending requests. The separate connection read timeout can
still close an inactive connection.

`SQL_FUN_SERVER` names an executable path, never a shell command. Relative paths
are resolved from the caller's current working directory. If it is unset,
`sql-fun-server` is located on `PATH`. An invalid explicit setting is not replaced
with a PATH candidate.

`--server-command` overrides `SQL_FUN_SERVER_COMMAND`. This optional launch-only
command runs through `/bin/sh -c` in the caller's current working directory;
`/bin/sh` must be available when using it. The executable reference is still
required. The launch command is not executed when an existing server can be
reused. Without an override, the selected executable is launched directly.

For repository development, `.envrc` sets:

```sh
export SQL_FUN_SERVER="${GIT_ROOT}/target/debug/sql-fun-server"
export SQL_FUN_SERVER_COMMAND='cargo run -p sql-fun-server'
```

You can also supply the Cargo command explicitly:

```sh
sql-fun api --user alice --server-command 'cargo run -p sql-fun-server'
```

This requires Cargo and access to the selected project from the caller's working
directory. Startup compilation counts toward the default 40-second connection
budget. Prebuild with `cargo build -p sql-fun-server` when compilation would
exceed that budget. The Cargo command is a repository development setting;
installed CLI usage does not require Cargo. The CLI does not search for project
files or read project metadata to choose a server.

Malformed, stale, unauthenticated, or unresponsive registrations fail without
stopping the server or deleting its registration. Automatic registration
recovery is not currently supported.

## Input, output, and completion

Each input line must be a complete JSON object with a string `method` and valid
`params` for that API method. Do not supply an `id`, even `null`: the client
allocates IDs. You can omit `jsonrpc`; when supplied, it must be exactly the
string `"2.0"`. An explicitly invalid version is rejected.

`echo` requires `params` to be an object or array. Both can contain nested JSON
values, including null. Scalar or null parameters are rejected. Other accepted
method names are `initialize_schema_context`, `read_collection`,
`release_collection`, `tokenize_sql`, `analyze_query`, and `describe_table`,
with parameters matching their public API argument types.
The server dispatches `echo`, `tokenize_sql`, `initialize_schema_context`,
`read_collection`, `release_collection`, and `analyze_query`. The CLI accepts
`describe_table`, but the server does not dispatch it and returns an
unknown-method error. See the
[schema context and query analysis workflow](check-analysis-design.md) for
initialization and analysis examples.

Requests can remain outstanding while subsequent lines are submitted. Slow
output applies backpressure. Stdout contains only complete JSON-RPC response
objects, one per line, including server error responses. Match responses by ID;
response order is not guaranteed to match submission order. Diagnostics and
logs go to stderr. Use `RUST_LOG=info` to enable request/response tracing.

At stdin EOF the CLI drains accepted requests and their output before exiting
successfully. A malformed or invalid line stops further input, reports its line
number on stderr, and drains preceding accepted requests before exiting with a
nonzero status. Input read failures, transport failures, and stdout write or
flush failures also produce a nonzero status. They are not synthetic server
responses. A server JSON-RPC error response by itself does not indicate a CLI
transport failure.

Draining has no implicit timeout. The command waits for outstanding responses
and output completion; keep the output consumer reading until EOF.

## SQL tokenization

`tokenize_sql` needs the original SQL string and an explicit dialect; it does
not read project files or require a schema context. For example:

```sh
printf '%s\n' \
  '{"method":"tokenize_sql","params":{"source":"BEGIN","dialect":"postgres"}}' |
  sql-fun api --user alice
```

Use `"postgres"` for PostgreSQL or `"SQLite"` when the CLI and server are built
with SQLite support. Missing or unavailable dialects are errors; the server does
not select another lexer. Invalid SQL syntax or unknown database objects do not
prevent tokenization when the selected lexer accepts the text. Lexical errors
return a JSON-RPC error with `TokenizationFailed` in its `data`.

The result is a collection page. Each item contains `kind`, `start`, and `end`;
`start` is inclusive and `end` is exclusive, measured in UTF-8 bytes from the
beginning of the original request source. Categories are `identifier`,
`constant`, `operator`, `keyword`, `comment`, `trivia`, and `delimiter`. Ordered
ranges cover all original SQL bytes, including whitespace and comments. The
response does not repeat the SQL text; retain the original source to extract
each token's spelling.

The raw API command returns pages without collecting or releasing them. Keep the
same API connection open to send `read_collection` requests using the returned
`handle` and successive `start_offset` values until `total_length` items have
been read. If `require_release` is true, send `release_collection` with that
`collection_handle`, including for an empty collection or after the last page.
Handles belong to that connection and cannot be reused by a later CLI
invocation.

Rust callers preparing diagnostics can instead use
`TcpJsonRpcClientAsync::tokenize_sql(source, dialect)`. This explicitly requests
all pages and releases the retained collection before returning a validated core
`TokenStream`. A read, decode, release, or invalid-range failure returns an
error rather than a partial stream. CLI diagnostic preparation awaits this
operation; rendering and highlighting remain synchronous.

## Response size limitation

The authenticated client buffers complete JSON-RPC response frames and does not
yet provide a configurable maximum receive size through its API or CLI
([#781](https://github.com/kazuk/sql-fun/issues/781)). Large analysis responses,
including executable SQL, can therefore consume correspondingly large memory.
The proposed 4 MiB default in that issue is not an implemented limit or current
configuration option. Authentication has a separate bound; it does not cap
subsequent authenticated response frames.
