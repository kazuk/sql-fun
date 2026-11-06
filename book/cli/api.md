# `sql-fun api`

Stream JSON Lines requests to the authenticated per-user `sql-fun-server`:

```text
sql-fun [COMMON OPTIONS] api [OPTIONS]
```

Install `sql-fun-server` and make it available on `PATH`, or set
`SQL_FUN_SERVER` to its executable path. A valid user namespace is required.
The [common options](index.md#common-options) precede `api`; project and schema
settings are not used by the bridge.

| Option | Environment | Behavior |
| --- | --- | --- |
| `-u`, `--user <USER>` | `USER` | Required through the option or environment; the option takes precedence. Selects the server namespace. |
| `--server-command <COMMAND>` | `SQL_FUN_SERVER_COMMAND` | Optional launch-only command; the option takes precedence. It runs through `/bin/sh -c` in the current directory if a server must be started. |
| `-h`, `--help` | — | Print help. |

`SQL_FUN_SERVER` selects an executable path, resolved relative to the current
working directory if necessary. If unset, `sql-fun-server` is located on `PATH`.
The executable reference is required even with a launch command. A running server
for the selected user is reused; otherwise the executable or launch command is
used to start one. `RUST_LOG` controls bridge tracing to stderr. `SQL_FUN_SERVER_IDLE_TIMEOUT`
configures the idle timeout in seconds for a newly launched server; its default
is 180, and `0` disables idle shutdown. It does not reconfigure a reused server.

```sh
printf '%s\n' '{"method":"echo","params":{"hello":"world"}}' |
  sql-fun api --user alice
```

Input and responses are JSON objects, one per line. The CLI assigns request IDs;
responses can arrive in a different order. Standard output carries responses;
diagnostics and logs go to stderr. The command exits successfully after EOF and
after draining accepted requests. Invalid input, connection, transport, or output
errors produce a nonzero exit; a JSON-RPC error response alone does not. Invalid
CLI arguments exit with status 2.

For request formats, method examples, connection and session lifecycle, see
[Using the Server API via `sql-fun api`](../api/cli-api.md).
