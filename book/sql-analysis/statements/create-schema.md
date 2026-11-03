# CREATE SCHEMA

## Overview

PostgreSQL CREATE SCHEMA records a schema identity and can analyze nested
supported declarations in a staged schema group. Nested declarations use the new
schema at the front of the search path; the preceding path is restored
afterward. Supported IF NOT EXISTS preserves an existing schema without applying
nested elements. An explicitly supplied AUTHORIZATION owner uses the role lookup
described at [ALTER … OWNER TO](alter-owner.md).

```sql
CREATE SCHEMA application;
```

The group is catalog preparation for later analysis, not execution of a database
transaction. Rejected nested creations leave the staged changes unpublished
rather than exposing a partial schema group.

## Dialect and engine-specific behavior

This is PostgreSQL schema analysis. The supported nested element set follows
each statement's own semantic/application boundaries. It does not make arbitrary
utility grammar inside CREATE SCHEMA supported. SQLite's attached-database
namespaces do not implement PostgreSQL CREATE SCHEMA semantics.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): the schema
  identity or an analyzed nested declaration conflicts with an existing
  identity.
- [ownership_role_not_found](../../diagnostics/ownership_role_not_found.md): an
  explicit named authorization role cannot be resolved.

Nested statements retain their own diagnostic codes.
