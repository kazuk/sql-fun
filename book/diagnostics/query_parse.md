# query_parse

## Code and severity

Code: `query_parse`. Severity: Error.

## When it is reported

SQL input cannot be parsed, or preparation of the analyzed statement fails in
the parse stage.

## What analysis knows and does not know

The stage-specific explanation and available source context are retained. No
complete semantic analysis result is established for that failed input.

## How to address it

Check syntax for the explicitly selected dialect and engine version. If valid
dialect syntax is rejected, preserve a minimal example when reporting parser
support rather than treating the diagnostic as a database verdict.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. The parser path
also supports SQLite input in appropriate API contexts, but each dialect has its
own syntax.

```sql
SELECT FROM;
```

## Related diagnostics

[unsupported_statement](unsupported_statement.md) is a non-syntax
parser/conversion capability failure; [query_analysis](query_analysis.md) occurs
later in analysis.
