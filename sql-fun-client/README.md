# `sql-fun-client`

Connect to a per-user `sql-fun-server` on the same Linux host. Callers explicitly
choose an absolute server executable and a user namespace. The client reads
`/tmp/<user>/sql-fun-server.pid`, starts a detached server only when registration
is absent, and authenticates before returning a connection. The user must be a
single nonempty path component; it is also passed as `USER` to launched commands.

```rust,no_run
use std::path::Path;
use sql_fun_client::{ConnectionOptions, connect};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let options = ConnectionOptions::new(
    Path::new("/opt/sql-fun/bin/sql-fun-server"),
    "alice",
    None,
    None,
);
let client = connect(&options).await?;
println!("{}", client.peer_addr()?);
drop(client);
# Ok(())
# }
```

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

The self-contained example demonstrates the public connection API:

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
Do not act on an unverified PID. Safe automatic recovery is tracked in issue #415;
atomic server registration publication is a separate improvement in issue #407.
Until that improvement lands, discovery during the brief registration-write
window can fail with a malformed-record error. This is an accepted MVP limitation;
it does not prevent normal launch, authentication or reuse once writing completes.

The async low-level `try_from` wraps a socket without authentication; use `connect`
for the authenticated lifecycle. `invoke_method` accepts JSON object parameters
and bounds response frames to 2 MiB. Calls after initialization have no implicit
time deadline; callers can use `tokio::time::timeout`. Cancellation or a
transport/protocol failure closes the socket to prevent a partial exchange from
corrupting later calls.

Integration tests run in the standard workspace nextest suite. Their heavy
fixtures prebuild the actual server in the dev profile with matching
SQLite features before timing any connection, and use unique fixture-owned user
namespaces. They call the library directly and do not execute the example.
The example is compiled by the standard lint step's all-targets check.
No developer registration is used.
