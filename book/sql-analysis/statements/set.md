# SET

## Overview

The current PostgreSQL variable-setting analysis resolves a single supplied
expression and records the variable name/value in the analysis context.

```sql
SET statement_timeout = 0;
```

This is retained configuration metadata. It does not configure a live session,
discover project metadata, or implicitly select an engine/datasource. Select the
analysis environment through the explicit [configuration
inputs](../../installation.md).

## Dialect and engine-specific behavior

The semantic path requires exactly one argument. General RESET, DEFAULT, FROM
CURRENT, transaction options and multiargument setting forms may be retained by
syntax but are not completed by that path. Recording a setting is not evidence
that all effects on PostgreSQL semantics are modeled. SQLite PRAGMA and
attachment changes are separate analysis boundaries; in particular
rename-related PRAGMA behavior remains unmodeled.

SQLite/Turso PRAGMA, ATTACH and DETACH analysis remains unimplemented; see the
[statement support
inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).
These paths do not acquire PostgreSQL semantics from similarly named commands.

## Diagnostics

The single-expression path can emit the shared expression/name/type diagnostics.
It does not define a setting-specific diagnostic code or validate the complete
PostgreSQL setting catalog. Unsupported argument shapes can fail analysis rather
than emitting a new structured setting diagnostic.
