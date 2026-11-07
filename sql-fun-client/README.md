# `sql-fun-client`

A Rust client for the [`sql-fun-server`](https://crates.io/crates/sql-fun-server)
JSON-RPC service. See the [Rust API documentation](https://docs.rs/sql-fun-client)
and the [Server API Reference](https://kazuk.github.io/sql-fun/api/server-api.html).

Connect to a per-user `sql-fun-server` on the same Linux host. Callers explicitly
choose an absolute server executable and a user namespace. The client reads
`/tmp/<user>/sql-fun-server.pid`, starts a detached server only when registration
is absent, and authenticates before returning a connection. The user must be a
single nonempty path component; it is also passed as `USER` to launched commands.

```rust
use sql_fun_client::{ConnectionOptions, connect};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let options = ConnectionOptions::builder()
        .server_executable("/opt/sql-fun/bin/sql-fun-server")
        .user("alice")
        .build();
    let client = connect(&options).await?;
    println!("{}", client.peer_addr()?);
    drop(client);
    Ok(())
}
```

The builder requires both executable and user before `.build()`; its path and
string setters accept borrowed values without manual conversions. Optional
`.server_command("…")` and `.timeout(Duration::from_secs(10))` setters override
the launch command and budget. For conditional inputs, use
`.maybe_server_command(command)` and `.maybe_timeout(timeout)`.
`ConnectionOptions::new` remains available as a convenience constructor.

A Tokio runtime and a Linux environment with `/bin/sh` for custom commands are
required. The default command executes the supplied binary directly. The client
does not search PATH or read Cargo project metadata to select that binary.
Servers run in a separate session with standard input/output/error disconnected
from the caller. Dropping a connection closes its socket; the server persists.

The optional command is executed using `/bin/sh -c` in the caller's current
working directory, with the caller's environment and the selected `USER`.
Commands must start the intended server using that namespace. Shell quoting and
metacharacters have their usual meaning. A healthy server is reused before the
command is executed, so even a broken override is ignored when reuse succeeds.

`None` timeout means 40 seconds. A positive duration changes the total budget;
zero timeout, a relative executable, or an invalid user fails before any
registration access or launch. Discovery, launch, connect, initialization and
retries share one monotonic deadline. Each connection/initialization attempt is
also bounded. Initialization validates the JSON-RPC version, matching request ID,
and `result: "OK"`. TCP connection alone does not establish readiness.

The repository contains a self-contained example of the public connection API.
Run it from a repository checkout:

```sh
cargo build -p sql-fun-server
cargo run -p sql-fun-client --example sql-fun-client-connect -- \
  --server-executable "$PWD/target/debug/sql-fun-server" \
  --user "$USER" --timeout-ms 40000
```

For a Cargo startup override, install Cargo/Rust and prebuild the server with
matching features/profile first, then add
`--server-command 'cargo run -p sql-fun-server'`. Compilation time is part of the
connection deadline if the server was not prebuilt. The example requires an explicit
`--user`; omitted command and timeout use the documented behavior above.

Malformed or incomplete registrations fail immediately and are preserved.
Unreachable servers, failed initialization and stalled handshakes receive
bounded retries with registration rereads. A replacement registration can be
followed within the same deadline. Errors report the observable stage without
printing tokens; initialization EOF cannot establish that the token was rejected.

Automatic recovery is unsupported in this MVP. An unusable existing registration
is preserved: the client never signals the registered process, deletes its file,
or launches a replacement over it. Crash leftovers, zombies and inconsistent
tokens can therefore require operator-assisted recovery. Verify the intended
server and registration before cleanup and stop other startups during that work.
Do not act on an unverified PID. Safe automatic recovery is tracked in [issue #415](https://github.com/kazuk/sql-fun/issues/415).

The async low-level `try_from` wraps a socket without authentication; use `connect`
for the authenticated lifecycle. `invoke_method` accepts protocol-compatible JSON
parameters and an asynchronous callback receiving a `JsonRpcResponse`. Submission
returns when the client accepts the request, before its response arrives. The
client allocates IDs and matches responses to callbacks, including responses
arriving out of order. Valid JSON-RPC error responses are delivered to callbacks.
Use `response.into_result()` to handle server errors or serialize the response
with Serde to retain its JSON-RPC envelope and ID.

```rust
async fn example(
    mut client: sql_fun_client::TcpJsonRpcClientAsync,
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .invoke_method(
            "echo",
            &serde_json::json!({"message": "hello"}),
            |response| async move {
                // Handle application failures here; the client cannot receive them.
                match response.into_result() {
                    Ok(value) => println!("{value}"),
                    Err(error) => eprintln!("{error}"),
                }
                Ok(())
            },
        )
        .await?;
    client.finish().await?;
    Ok(())
}
```

Callbacks own their responses, must be `Send + 'static`, and return
`Result<(), Infallible>`. Up to 8 accepted user requests can remain unfinished,
including callbacks still running. Further submissions wait for capacity.
Callbacks must not wait for additional submissions to the same saturated client.
Callback order is unspecified, so applications needing serialized output should
coordinate it themselves. Application output/storage errors are handled inside
the callback; the client does not collect or report them.

`finish(self)` stops new user submissions and completes accepted typed responses,
including their reads/releases, before half-closing TCP writes. It then drains
responses through EOF and waits for callback completion. EOF with missing responses
is an error. Transport/protocol failures also surface through subsequent submissions
and completion. `wait_for_failure()` permits observing terminal failure while
waiting for further application input. Calls after
initialization have no implicit deadline. Callers can apply
`tokio::time::timeout`. Cancelling submission before acceptance leaves no pending
request; dropping the client or cancelling `finish` closes its socket/tasks.
Neither completion nor drop stops the shared server.

Authenticated responses have a **4 MiB (4,194,304 bytes)** default receive limit.
Use `.max_response_bytes(8 * 1024 * 1024)` on `ConnectionOptions::builder()` to
raise it, or `.maybe_max_response_bytes(limit)` for an optional `u32`. Existing
`ConnectionOptions::new(...)` and `TcpJsonRpcClientAsync::try_from(stream)` use
the default. Low-level socket users can select a limit with
`TcpJsonRpcClientAsync::try_from_with_max_response_bytes(stream, limit)`; this
constructor does not authenticate. The independent authentication-response bound
remains 8,192 bytes.

The range is `0..=u32::MAX` (4,294,967,295 bytes), including values below 4 MiB.
Zero rejects all nonempty authenticated frames; it does not mean unlimited.
The limit counts the entire JSON-RPC frame's wire bytes, including JSON escaping,
metadata, executable SQL, and the terminating newline. On overflow, the client
reports a transport receive-size error with the configured limit and a lower
bound on observed bytes, then closes the connection. It cannot continue or
resume that session. Explicitly increase the limit and open a new connection to
retry a valid large response; no SQL change is required.

The client grows its buffer as data arrives without preallocating the configured
maximum, but still buffers each complete response. Larger limits permit greater
memory use. JSON decoding, concurrent/queued responses, and typed collection
materialization add allocations, so this is not a total-process memory cap.

Use `invoke_typed` to explicitly select automatic completion of client-owned typed
responses. Supply the method, parameters, and result type; this API never infers
collection handles from JSON keys. The original callback receives exactly one
`Result<R, TypedResponseError>` after all collections, including those nested in
items, have been read and any required releases have succeeded. Submission errors
before acceptance return directly and do not invoke the callback.

```rust
use sql_fun_client::payloads::SchemaContext;

async fn example(
    mut client: sql_fun_client::TcpJsonRpcClientAsync,
    params: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .invoke_typed(
            "initialize_schema_context",
            &params,
            |response: Result<SchemaContext, _>| async move {
                match response {
                    Ok(context) => {
                        println!("{} diagnostics", context.problems().items().len());
                    }
                    Err(error) => eprintln!("{error}"),
                }
                Ok(())
            },
        )
        .await?;
    client.finish().await?;
    Ok(())
}
```

Typed completion materializes the **entire collection in client memory**. A
wire collection deserializes into client `Collection::Stream`; successful
completion produces `Collection::Completed`, whose `items()` contains the full
ordered data and whose `handle()` is `None`. Completed state is client-only:
serialization returns an error and its variant is excluded from the wire
schema. `Stream` represents incomplete state, without a public lazy stream or
iterator contract. Initial nonzero offsets are supported by reading the missing
prefix and suffix. Positive short pages are continued; zero progress or
inconsistent pages fail. Empty and initially complete retained collections
still require release before callback delivery.

All follow-up calls use the original authenticated connection. Internal RPCs have
24 reserved admission slots and share the existing bounded queue with the 8 user
slots, allowing progress when all user slots await typed completion. Manually
submitted read/release calls use user slots. No queue scheduling priority is
promised. Use `invoke_method` for manual paging or opaque JSON results; selecting
`serde_json::Value` with `invoke_typed` treats it as an opaque leaf.

The library provides `CompleteResponse` for API response/item types, including
opaque `EchoArgs` results, and for `Option<T>`, `Vec<T>`, and `Box<T>` composition.
For custom response types, implement `payloads::CollectionTraversal` to enumerate
collection fields as type-erased `payloads::CollectionReference` references;
`CompleteResponse` is then supplied automatically. Alternatively, implement
`CompleteResponse` to register known handles and delegate completion explicitly.
Unknown types are never silently skipped.

On failure, the original callback receives an error rather than a partially
completed result. While the connection remains usable, completion attempts to
release known retained handles, including decoded sibling collections. Cleanup
failures are included with the original error. A release whose outcome may be
uncertain is not retried. Cleanup cannot guarantee release after disconnect,
client drop/cancellation, undecodable initial responses, invalid handle metadata,
or unread nested items. Dropping the client or cancelling `finish` closes the
session and aborts its tasks; cleanup does not run in detached background tasks.
No response completion can resume on a different connection.

The client emits `tracing` INFO events after a request is written (`request_id`,
`method`) and when a response is received (`request_id`, and `error_code` for
JSON-RPC error responses). Response events omit `method`. Parameters, payloads,
and authentication tokens are never recorded by the client. The consuming
application installs/configures its own tracing subscriber.
