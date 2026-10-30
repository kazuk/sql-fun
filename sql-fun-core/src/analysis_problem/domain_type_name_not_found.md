# Domain base type not found : `domain_base_type_not_found`

Official Document: [CREATE DOMAIN statement](https://www.postgresql.org/docs/current/sql-createdomain.html)

## Situation

```SQL
CREATE DOMAIN name AS base_type;
```

- `base_type` not found in schema semantic analysis context.

## Why

- Typo in `base_type`.
- `PostgreSQL` extension not enabled in context.
- When `base_type` defined in schema
  - Schema not specified.
  - Schema not in search path.
