# query_deparse

## Code and severity

Code: `query_deparse`. Severity: Error.

## When it is reported

The parsed or rewritten statement cannot be rendered back into executable SQL
through the supported deparser.

## What analysis knows and does not know

The deparse-stage explanation is retained. Other statement facts or diagnostics
may still be available, but executable SQL can be unavailable.

## How to address it

Check the reported unsupported rendering construct and the selected dialect.
Preserve a minimal SQL example when reporting the limitation; valid input need
not be rewritten unless an equivalent supported form meets your needs.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This currently
reports `query_deparse` as well as a rule-target/identity failure. Deparse
support differs by statement and dialect.

```sql
DROP RULE missing ON public.items;
```

## Related diagnostics

[unsupported_statement](unsupported_statement.md) cannot complete earlier
statement conversion; [query_analysis](query_analysis.md) concerns later
semantic or access processing.
