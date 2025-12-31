# initialize method

JSON-RPC initialization method.  

This server supports both MCP and LSP clients.

## Request examples

### LSP Client

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "processId": 1234,
    "clientInfo": { "name": "vscode", "version": "1.89" },
    "capabilities": { ... },
    ...
  }
}
```

### MCP Client


```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-03-26",
    "clientInfo": { "name": "cline", "version": "0.3.1" },
    "capabilities": {
      "tools": {},
      "resources": {},
      "prompts": {}
    }
  }
}
```

## Response example

The server returns combined capabilities for both `LSP` and `MCP`.

The `methods` key lists `sql-fun` specific JSON-RPC methods.
To inspect each method’s request and response schema, use the `sql-fun/help-method`.


```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "capabilities": {
      "textDocumentSync": 2,
      "completionProvider": { "resolveProvider": true },
      "tools": {
        "list": true,
        "describe": true
      },
      "methods": [
        {"name": "sql-fun/help-method/1","version": "1.0.0"},
        {"name": "sql-fun/list/1", "version": "1.2.3"},
        {"name": "sql-fun/describe/1", "version": "1.2.3"},
        {"name": "sql-fun/describe/2", "version": "2.0.0", "experimental": true}
        {"name": "sql-fun/validate/1", "version": "1.2.3"}
      ]
    }
  }
}
```

### Method Entry Format

Each entry in the methods array contains:

- name: The full method name, including version.
- version: Semantic version (semver) of the method implementation.
- experimental (optional): Whether the method is experimental. Defaults to false.

Methods marked with "experimental": true are provided for evaluation and feedback purposes only.
These methods are subject to change or removal at any time without prior notice.
Stability, compatibility, and long-term support are not guaranteed.

You are welcome to explore and test these methods, but use them at your own risk.
We recommend not relying on experimental methods in production environments.

### Method Naming Convention

Method names follow this format:

```
sql-fun/{method-name}/{major}[Experimental suffix]
```

- `sql-fun` is the namespace prefix.
- `{method-name}` is the logical name of the method.
- `{major}` is the major version number.
- `[suffix]` *(optional)*: Variant identifier for experimental or pre-release versions (e.g., `-exp`, `-rc1`, `-beta3`).

Suffixes are used for unstable variants and **must be marked with `"experimental": true`** in the method entry.

Example:

```json
 {"name": "sql-fun/describe/2", "version": "2.0.0"},
 {"name": "sql-fun/describe/2-exp", "version": "2.1.0"}
```

In this case:

- `sql-fun/describe/2` refers to the stable 2.0.0 implementation.
- `sql-fun/describe/2-exp` refers to the experimental 2.1.0 implementation.

Use `sql-fun/help-method/1` to obtain the schema of any listed method.

