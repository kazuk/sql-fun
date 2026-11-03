# ALTER … OWNER TO

## Overview

ALTER OWNER analysis resolves named owners against the selected datasource's
explicit [`roles`
metadata](../../installation.md#configure-known-postgresql-roles). For example,
`roles = ["app_owner"]` allows `ALTER SCHEMA app OWNER TO app_owner` when schema
`app` exists. The analyzer retains the resolved role and updates the schema
ownership used by later analysis. Other supported ownership targets use the same
role lookup.

SQL unquoted names fold to lowercase; quoted names preserve their exact case and
escaped quotes. Configured strings are exact catalog names, not SQL expressions.
An unavailable name produces `ownership_role_not_found`; the rejected ownership
change leaves the catalog unchanged, and later statements can still be analyzed.
Omitted configuration and an explicit empty list both leave named roles
unavailable. SQL `CURRENT_ROLE`, `CURRENT_USER`, and `SESSION_USER` remain
symbolic session-dependent owners; no concrete session role is inferred. Quoted
names such as `"CURRENT_USER"` instead request a named role from the inventory.

This is offline semantic analysis of known role identities. It does not check
whether a connected user could transfer ownership or grant database permissions.

## Dialect and engine-specific behavior

This is PostgreSQL offline ownership analysis. Supported object kinds resolve
their identities and, for overloaded objects, supplied argument type identities
before applying an ownership change. The existence of an ALTER OWNER grammar
form does not establish that every PostgreSQL object kind is implemented. SQLite
has no corresponding PostgreSQL role/owner model. Role configuration remains
documented in
[Installation](../../installation.md#configure-known-postgresql-roles).

## Diagnostics

- [ownership_role_not_found](../../diagnostics/ownership_role_not_found.md): a
  requested named role is absent from the explicit role inventory.
- [ownership_argument_type_not_found](../../diagnostics/ownership_argument_type_not_found.md):
  an object-signature argument type cannot be resolved.
- [ownership_invalid_target_kind](../../diagnostics/ownership_invalid_target_kind.md):
  the resolved target has an incompatible object kind.
- [ownership_target_not_found](../../diagnostics/ownership_target_not_found.md):
  the requested object identity is absent.
- [ownership_target_ambiguous](../../diagnostics/ownership_target_ambiguous.md):
  the request does not uniquely identify an overloaded target.
