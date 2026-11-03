# unsupported_statement

## Code and severity

Code: `unsupported_statement`. Severity: Error.

## When it is reported

The parser/conversion path recognizes input that cannot be converted into a
supported statement representation, rather than classifying the failure as a
syntax error.

## What analysis knows and does not know

The conversion explanation is retained. The diagnostic reports a capability
boundary and does not establish that the database rejects the SQL.

## How to address it

Check supported analysis coverage for the selected dialect. Use an equivalent
supported form only when appropriate; otherwise report the minimal valid
construct whose support is needed.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. PostgreSQL accepts
VACUUM, but query analysis does not support every administrative statement.
SQLite has separate parser/conversion coverage.

```sql
VACUUM;
```

## Related diagnostics

[query_parse](query_parse.md) classifies syntax-stage failure;
[query_deparse](query_deparse.md) concerns rendering an already parsed
statement.
