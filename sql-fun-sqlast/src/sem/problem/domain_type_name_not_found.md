# Domain base type not found : SF00001

Official Document: [CREATE DOMAIN statement](https://www.postgresql.org/docs/current/sql-createdomain.html)

## Situation

```SQL
CREATE DOMAIN name AS base_type;
```

- `base_type` not found in schema analysis context.

## Why

- Typo in `base_type`.
- PostgreSQL extension not enabled in context.
- When `base_type` defined in schema
  - Schema not specified.
  - Schema not in search path.
