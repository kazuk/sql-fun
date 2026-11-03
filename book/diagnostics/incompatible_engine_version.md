# incompatible_engine_version

## Code and severity

Code: `incompatible_engine_version`. Severity: Error.

## When it is reported

Recognized syntax requires PostgreSQL 18 or later, but the explicitly selected
engine version does not establish that capability. This includes unavailable or
unparsable version information.

## What analysis knows and does not know

The construct, minimum version and selected version when available are retained.
Parser recognition alone does not authorize treating the target engine as
PostgreSQL 18.

## How to address it

Select the actual target version explicitly and use syntax supported by that
target. Do not change the configured version merely to suppress a real
deployment incompatibility.

## Example

PostgreSQL 17 target with a loaded `migration_rows(id integer)` table. OLD/NEW
RETURNING row images require PostgreSQL 18. Other gated constructs include
virtual generated columns, temporal constraints and new constraint syntax.

```sql
DELETE FROM migration_rows RETURNING old.*;
```

## Related diagnostics

[alter_column_version](alter_column_version.md) concerns a specific ALTER COLUMN
version restriction; [unsupported_statement](unsupported_statement.md) concerns
analyzer coverage rather than target-version support.
