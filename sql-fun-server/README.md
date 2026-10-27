# sql-fun-server

JSON RPC server for `sql-fun`

## Security Notice

This software does not implement support for Windows-specific security mechanisms.
Running on Windows may result in degraded security.

Use at your own discretion.

## Idle shutdown

The server exits successfully after **180 seconds** without connected clients,
releasing its in-memory schema cache and removing its PID registration. The idle
period begins at startup and again after the last client disconnects. A new
connection cancels the pending shutdown; an open connection prevents server idle
shutdown even when no requests are in progress.

Set `--server-idle-timeout` or `SQL_FUN_SERVER_IDLE_TIMEOUT` to change the period
in seconds. The command-line option takes precedence over the environment
variable. **0 disables automatic idle shutdown.** Values must be integers from 0
to 4294967295; invalid values are rejected at startup.

```sh
sql-fun-server --user alice --server-idle-timeout 600
# Or, for a newly launched server:
SQL_FUN_SERVER_IDLE_TIMEOUT=0 sql-fun-server --user alice
```

Configuration is read at server startup. Changing the environment of a later CLI
invocation does not reconfigure an already running shared server.

The existing `--idle-timeout` / `IDLE_TIMEOUT` setting remains a separate
per-connection read timeout (default: 30 seconds). It can close an inactive
connection, after which the server-wide idle period starts if no clients remain.
Authentication timeouts remain independent as well.
