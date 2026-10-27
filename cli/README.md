# sql-fun Command Line Interface

## Installation

```console
cargo install sql-fun-cli
cargo install --locked cargo-expand
cargo expand --version
```

`sql-fun check` requires `cargo-expand`. Installing `sql-fun-cli` does not
install this tool automatically. Ensure Cargo's binary directory is on `PATH`
in local and CI environments.

See the [installation guide](../book/installation.md) for server installation,
SQLite feature flags, and project configuration.

## `sql-fun` CLI Common arguments and environment variables

### Precedence

`command-line option` **>** environment variable **>** project metadata **>**
built-in default.

If both CLI option and environment variable are provided, the CLI option wins.

### Options & environment mappings

| CLI option | Environment | Default | Description |
|---|---|---|---|
| `--help` | *(none)* | *(n/a)* | Show usage help and exit. |
| `--metadata-format <FMT>` | `SQL_FUN_METADATA_FORMAT` | `TOML` | Metadata file format. **Currently only `TOML` is supported.** |
| `--metadata-file <PATH>` | `SQL_FUN_METADATA_FILE` | `Cargo.toml` (when format is `TOML`) | File that contains `[package.metadata.database]`. If **absolute path**, `sql-fun` checks only that path (**no upward searching**). If **relative**, the file is resolved against the discovered project root (see below). |
| `--schema <ID>` | `SQL_FUN_SCHEMA` | `develop` | Schema selector. Ignored when `--schema-file` is supplied. |
| `--schema-file <PATH>` | `SQL_FUN_SCHEMA_FILE` | *(none)* | Specify schema dump file used by the application. **If this is supplied, the schema selector is ignored.** |
| `--cte-catalog-dir <DIR>` | `SQL_FUN_CTE_CATALOG_DIR` | from metadata (`[package.metadata.database].cte_catalog_dir`) | Directory for the CTE catalog. Overrides the metadata value when provided. |
| `--sql-dialect <NAME>` | `SQL_FUN_DIALECT` | `postgresql` | SQL dialect for parsing and AST. **Currently only `postgresql` is supported** (via `pg_query`). |
| `--output-format <FMT>` | `SQL_FUN_OUTPUT_FORMAT` | `text` | Output format for the parent CLI. Supported values: `text`, `json`. Internally the system uses JSON or JSON-serializable data; `--output-format json` prints raw JSON. |
| `--verbose` | *(none)* | *(n/a)* | Equivalent to `--log-level=debug`. |
| `--log-level <LEVEL>` | `SQL_FUN_LOG_LEVEL` | `info` | Logging level. One of: `trace`, `debug`, `info`, `warn`, `error`. |
| `--log-file <PATH>` | `SQL_FUN_LOG_FILE` | *(stderr)* | Log file path. If the path exists, logs are appended; otherwise the file is created. |

### Postgres version and Built-in table / view options

**Default location**: `~/.sqlfun/schema/` with file name
`postgres[version].sql`  
(e.g., PostgreSQL 17 → `~/.sqlfun/schema/postgres17.sql`)

CLI option | Environment | Default | Description
---|---|---|---
`--postgres-version <version>` | `SQL_FUN_POSTGRES_VERSION` | from metadata (`[package.metadata.database] engine_version`) | Select the base PostgreSQL version used to resolve built-in objects.
`--no-builtin-tables` | `SQL_FUN_NO_BUILTIN` | (n/a) | Disable loading the built-in tables/views.
`--builtin-schema-file <file>` | `SQL_FUN_BUILTIN_SCHEMA` | See above | Use the specified SQL file as the built-in schema instead of the default versioned file.

> Precedence: `--builtin-schema-file` takes priority over
> `--postgres-version`.  
> If `--no-builtin-tables` is set, no built-in schema is loaded regardless of
> the other options.

## PostgreSQL extensions options

**Default location**: `~/.sqlfun/postgres/[version]/extensions/` with file name
`[extension]--[version].sql`  
(e.g., `uuid-ossp` v1.1 for PostgreSQL 17 →
`~/.sqlfun/postgres/17/extensions/uuid-ossp--1.1.sql`)

CLI option | Environment | Default | Description
---|---|---|---
`--postgres-extensions` | `SQL_FUN_EXTENSIONS` | from metadata (`[package.metadata.database] extensions`) | Comma sepaleted name for `extension[@version]`. (Ex. `tablefunc@1.0, uuid-ossp@1.1`)
`--postgres-extensions-dir` | `SQL_FUN_EXTENSIONS_DIR` | See above | Searching extensions SQL file from specified directory.

- If a version is omitted in `name[@version]`, the daemon or loader may resolve
  to a **default** or **latest installed** version based on its own policy.
- Paths are treated as **absolute** if they start with `/`; otherwise they are
  resolved relative to the working directory or project root, depending on
  command semantics.
- All files are expected to be UTF-8 encoded SQL. Load order follows the list
  order after the base built-ins (unless `--no-builtin-tables` is set).

### Project root discovery (for relative metadata paths)

When `--metadata-file` is **relative** or omitted, `sql-fun` discovers the
project root directory (`SQL_FUN_ROOTDIR`) by **searching upward** from the
current working directory and
**stopping at the first `Cargo.toml` owned by the current user**. Paths are
canonicalized. The default metadata file is resolved under this directory.

> Note: When `--metadata-file` is an **absolute** path, discovery is skipped and
> *only* that path is checked.

### Environment summary (injected by parent CLI to sub-commands)

For completeness, the parent CLI may inject the following variables to external
sub-commands and daemons:

- `SQL_FUN_ROOTDIR` — absolute project root directory that contains `Cargo.toml`
  (discovered as above).
- `SQL_FUN_METADATA_FILE` — absolute path to the metadata file (`Cargo.toml` for
  `TOML` format).
- `SQL_FUN_SCHEMA` — effective schema selector.
- `SQL_FUN_DIALECT` — effective SQL dialect (e.g., `postgresql`).
- `SQL_FUN_OUTPUT_FORMAT` — effective output format (`text` or `json`).
- `SQL_FUN_LOG_LEVEL` / `SQL_FUN_LOG_FILE` — effective logging configuration.

The parent also sets daemon-related variables where applicable:

- `SQL_FUN_CACHE_DIR` — base cache directory (XDG on Unix, `%LOCALAPPDATA%` on
  Windows).
- `SQL_FUN_DAEMON_SOCK` — daemon endpoint (UDS path on Unix; named pipe on
  Windows).
- `SQLFUN_API` — minimum protocol level required by the parent.

## Extensibility Support

`sql-fun-cli` supports **external sub-commands** discovered on `PATH`.

- Executable names must be prefixed with `sqlfun-`.
- If the executable is suffixed with `-daemon`, it is treated as a
  **daemon variant** and communicates via the
  [sql-fun daemon protocol](#sql-fun-daemon-protocol).
- External sub-commands receive the
  [sql-fun sub-command environment variables](#sql-fun-sub-command-environment-variables).

### Resolution Order

When executing `sql-fun <sub> ...`, the parent CLI resolves the target as:

1. `sqlfun-<sub>-daemon` (daemon variant)
2. `sqlfun-<sub>` (regular variant)

Resolution policy may be switched by flags in the parent CLI (e.g.
`--mode prefer-daemon|prefer-regular|require-daemon`).

Both extension binary require support `--help` option for provides usages.

## `sql-fun` sub-command environment variables

The following environment variables are injected by the parent CLI:

- `SQL_FUN_ROOTDIR`  
  Absolute directory path containing the project `Cargo.toml`.  
  `sql-fun-cli` searches from the current working directory upward,
  **stopping at the first `Cargo.toml` whose owner matches the current user**.
  Paths are canonicalized.

- `SQL_FUN_METADATA_FILE`  
  Absolute path to the **TOML `Cargo.toml`** that contains
  `[package.metadata.database]`.

- `SQL_FUN_SCHEMA`  
  Schema selector (e.g. `develop`). Default is `develop`.

- `SQL_FUN_CACHE_DIR`  
  Base cache directory for sql-fun (XDG on Unix, `%LOCALAPPDATA%` on Windows)
  such as `${HOME}/.cache/sql-fun`.

- `SQL_FUN_DAEMON_SOCK`  
  Endpoint for daemon IPC.  
  Unix: absolute path to a Unix domain socket file (e.g.
  `${SQL_FUN_CACHE_DIR}/daemon.sock`).  
  Windows: Named pipe (e.g. `\\\\.\\pipe\\sql-fun\\daemon`).

- `SQLFUN_API`  
  **Minimal protocol level** required by the parent CLI (string; e.g. `"1"`).

Optionally honored:

- `RUST_LOG` (e.g. `info,sqlfun=debug`) for logging verbosity.

## `sql-fun` daemon protocol

A **daemon binary** is an external executable named `sqlfun-<name>-daemon`. It
must accept:

- `--detached` : background (detached) start
- `--stop` : graceful stop
- `--status` : print status JSON (see below)
- `--restart` : restart in place
- Common options: `--socket <path-or-name>`, `--schema <id>`,
  `--log-level <level>`

### Parent CLI control shims

- `sql-fun start <NAME>` → executes `sqlfun-<NAME>-daemon --detached`
- `sql-fun stop <NAME>`    → executes `sqlfun-<NAME>-daemon --stop`
- `sql-fun status <NAME>` → executes `sqlfun-<NAME>-daemon --status`
- `sql-fun restart <NAME>` → executes `sqlfun-<NAME>-daemon --restart`

### Command delegation flow

For `sql-fun <NAME> [args...]`:

1. The parent computes a **semantic context** from the working directory and
   environment (see below).
2. It runs `sqlfun-<NAME>-daemon --status` and selects a **matching endpoint**:
   - Requires `protocol` compatible with `json-rpc/sqlfun/server`.
   - Must match the computed semantic context.
3. If a matching endpoint is found and `accept-commands=true`, the parent sends
   `[args...]` via the [Daemon command](#daemon-command).
4. Otherwise, if `semantic-context-loader=true`, the parent performs
   [Semantic context loading](#semantic-context-loading) and **retries** the
   command.
5. If no endpoint can serve the request, the parent fails with a diagnostic.

### Status JSON (machine-readable)

The `--status` output MUST be JSON (UTF-8). Example:

```json
{
  "status": "running",
  "pid": 12345,
  "version": "1.0.0",
  "api": 1,
  "endpoints": {
    "unix:///home/user/.cache/sql-fun/daemon.sock": {
      "protocol": "json-rpc/sqlfun/server",
      "sql-dialect": "postgresql",
      "accept-commands": true,
      "semantic-context-loader": true,
      "semantic-context": [
        {
          "path": "/home/user/.sqlfun/schemas/postgres_17.sql",
          "hash": "DEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF",
          "mtime": "2025-08-01T12:34:56Z"
        },
        {
          "path": "/path/to/workspace/schema.develop.sql",
          "hash": "0123456789ABCDEF...",
          "mtime": "2025-08-10T00:00:00Z"
        }
      ]
    },
    "tcp://127.0.0.1:45451": {
      "protocol": "json-rpc/mcp/server",
      "sql-dialect": "postgresql",
      "accept-commands": false,
      "semantic-context-loader": false,
      "semantic-context": []
    }
  }
}
```

Fields:

- `status`: `"unknown" | "running" | "not-running"`
- `pid`: OS process id (if running)
- `version`: daemon version string
- `api`: supported daemon API level (integer)
- `endpoints`: map of **endpoint-URL** → details  
  - `protocol`: hierarchical protocol id (e.g. `json-rpc/sqlfun/server`,
    `json-rpc/mcp/server`)
  - `sql-dialect`: SQL dialect name
  - `accept-commands`: whether [Daemon command](#daemon-command) is supported
  - `semantic-context-loader`: whether
    [Semantic context loading](#semantic-context-loading) is supported
  - `semantic-context`: **ordered** list of loaded SQL sources (`path`,
    `hash`=SHA-256 hex, `mtime`=ISO-8601 UTC)

> The parent CLI may filter candidate endpoints by `protocol`, `sql-dialect`,
> and `semantic-context` match.

### Semantic context loading

**JSON-RPC 2.0** method exposed by endpoints with
`semantic-context-loader=true`.

- **Method**: `loadSchema`
- **Params**:

```json
{
  "sql_dialect": "postgresql",
  "schema_sql_files": ["/abs/path/schema.develop.sql", "/abs/path/ext/uuid-ossp.sql"]
}
```

- **Result** (`EndpointDescription`):

```json
{
  "reloaded": true,
  "endpoint": "unix:///home/user/.cache/sql-fun/daemon.sock",
  "semantic-context": [
    { "path": "...", "hash": "...", "mtime": "..." }
  ]
}
```

- **Errors** (`SchemaLoadingError`):
  - `code`:
    `"SCHEMA_NOT_FOUND" | "UNSUPPORTED_DIALECT" | "IO_ERROR" | "PARSE_ERROR" | "INTERNAL"`
  - `message`: human-readable text
  - `span` (optional): `{ "file": "...", "line": 123, "col": 4 }`

---

### Daemon command

**JSON-RPC 2.0** method exposed by endpoints with `accept-commands=true`.

- **Method**: `command`
- **Params**:

```json
{
  "args": ["--sql", "select * from users;"],
  "cwd": "/current/working/dir",
  "env": { "SQL_FUN_SCHEMA": "develop" }
}
``` 

- **Result**:

```json
{
  "code": 0,
  "stdout": "...\n",
  "stderr": ""
}
```

- **Errors**:
  - Standard JSON-RPC errors (`-32600`, `-32601`, `-32602`, `-32603`)
  - Domain errors (`"INVALID_ARGS"`, `"ENDPOINT_BUSY"`, `"INTERNAL"`)

#### Notes for implementers

- **Environment variables (MUST)**
  - `SQL_FUN_LOG_FILE`: Path to the log file. Create if missing; append if it
    exists.
  - `SQL_FUN_LOG_LEVEL`: One of `trace|debug|info|warn|error`.  
  Invalid values MUST fallback to `info`.

  Daemons **MUST honor** both variables; otherwise diagnostics will break.
- **Flush semantics (MUST)** All log records produced while handling a JSON-RPC
  request—especially `command`—**MUST be flushed before sending the response**.
  If flushing fails, emit an error to STDERR and continue.
- **Readiness**: after `--detached`, the daemon must create the socket/pipe and
  respond to `health` within a bounded time (e.g. 3s).  
  `health` result shape: `{"ok": true, "version": "...", "api": 1}`.
- **Security**: create socket with `0600` and containing dir with `0700` (Unix);
  validate and canonicalize schema file paths; limit request size.
- **Discovery**: parent CLI enumerates `PATH` and lists executables starting
  with `sqlfun-` (Windows considers `PATHEXT`).
- **Versioning**: parent sets `SQLFUN_API`, daemon advertises `api`; the parent
  rejects `api < required`.

##### Logging sink availability (MUST)

- **Detached mode** (`--detached`):
  - The daemon **MUST open a durable log sink before advertising readiness**
    (before creating/listening on the socket/pipe or returning a successful
    status).
  - The sink is the file pointed by `SQL_FUN_LOG_FILE`. If it is unset, use a
    deterministic default (e.g., `${SQL_FUN_CACHE_DIR}/daemon.log`).
  - If the sink **cannot be opened or written** (missing path, permission
    denied, disk full, etc.), **startup MUST fail**:
    - exit with a non-zero code (e.g., 70),
    - emit a single one-line diagnostic to the parent’s STDERR
      **before detaching**,
    - do **not** fall back to STDERR after detaching.
- **Foreground mode** (no `--detached`):
  - Fallback to STDERR is allowed, but SHOULD be explicitly noted in the first
    log line.
- **Writeability check (MUST)**:
  - Open the file with create+append semantics and perform an initial
    write/flush (“startup banner”) to verify the sink.
  - Use UTF-8 encoding; timestamps SHOULD be ISO-8601 UTC (`Z`).
- **Flush semantics (MUST)**:
  - All log records produced while handling a JSON-RPC request—especially
    `command`—**MUST be flushed before sending the response**.
- **Rotation/reopen (SHOULD)**:
  - Support log rotation (e.g., reopen on SIGHUP on Unix, or detect inode
    change).
- **Windows sharing (SHOULD)**:
  - Open the log with read-sharing to allow tailing tools.

> Rationale: in detached mode, STDERR is typically not visible or is discarded
> by the launcher; silently falling back breaks diagnostics. Failing fast
> ensures operators notice misconfiguration immediately.

##### Sensitive payload handling

- **MUST NOT** log plaintext credentials or personal data. This includes:
  - Connection strings/DSNs (e.g., `postgres://user:pass@host/db`,
    `password=...`).
  - Environment secrets (e.g., `PGPASSWORD`, `DATABASE_URL`, cloud tokens).
  - Raw sample rows or bind parameter values from queries.

- **MUST** apply redaction before writing logs:
  - URIs: replace the userinfo password part (e.g.,
    `postgres://user:***@host/db`).
  - Key-value DSN: mask `password=`, `sslpassword=`, `secret=` values.
  - Environment dumps: drop or mask known sensitive keys (allow-list preferred).

- **SHOULD** avoid logging full SQL text at `info` and above. If needed for
  debugging, log at `debug/trace` **after redaction** or log a
  normalized/parameterized form.

- **SHOULD** include correlation fields only (e.g., `rpc.id`, `rpc.method`) at
  higher levels.

- **Retention (SHOULD)** keep log rotation and minimal retention by default.
