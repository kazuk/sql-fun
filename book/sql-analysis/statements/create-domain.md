# CREATE DOMAIN

## Overview

PostgreSQL domain analysis resolves the base type, retains the domain's nominal
identity and category, and preserves the requested declaration modifiers and
explicit collation independently of the canonical base identity. A valid
definition can be used by later type and member-access analysis.

```sql
CREATE DOMAIN public.item_label AS text;
```

The shared [type rules](../expressions/types-and-casts.md) determine how a
domain participates in conversions; [COLLATE](../expressions/collate.md) owns
declaration validation. A missing base or invalid declaration collation does not
establish a usable domain definition.

## Dialect and engine-specific behavior

This is PostgreSQL domain metadata analysis. General domain-constraint analysis
remains incomplete ([#548](https://github.com/kazuk/sql-fun/issues/548));
retaining constraint syntax does not validate CHECK/default/runtime enforcement.
PostgreSQL 18 constraint features are version-gated and can reach explicit
unimplemented boundaries. SQLite type-name syntax does not supply this
PostgreSQL domain model.

## Diagnostics

- [domain_base_type_not_found](../../diagnostics/domain_base_type_not_found.md):
  the requested domain base type cannot be resolved.
- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  requested domain identity conflicts with an existing type/object.
- [incompatible_engine_version](../../diagnostics/incompatible_engine_version.md):
  a checked declaration feature requires a later explicitly selected PostgreSQL
  version.

Declaration collations can emit their shared lookup and capability diagnostics.
