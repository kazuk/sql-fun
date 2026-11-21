# invalid_privilege_declaration

## Code and severity

Code: `invalid_privilege_declaration`. Severity: Error.

## When it is reported

PostgreSQL GRANT or REVOKE names an unresolved catalog object, routine
signature, column, or role. It can also specify a privilege or membership
option that does not apply to the target or the selected PostgreSQL version.

## What analysis knows and does not know

The message identifies the unresolved name or inapplicable part of the
declaration. A valid PostgreSQL target may be absent from the selected
analysis catalog. Analysis does not evaluate the effective permissions of a
live session.

## How to address it

Check the selected schema, search path, role inventory, and PostgreSQL
version. Specify argument types to select an exact routine overload. Check
that the privilege applies to the target and any named columns. The
[statement reference](../sql-analysis/statements.md#other-postgresql-utility-statements)
lists targets that the shipped catalogs do not cover.

## Example

PostgreSQL 17 or 18. The selected project schema declares
`privilege_app.items (id integer, description text)` and the role inventory
contains `reader`.

```sql
REVOKE SELECT (missing) ON privilege_app.items FROM reader;
```

The table has no column named `missing`. Use an existing column, such as
`id`, or correct the schema input. CLI testing can also report
`query_deparse` because SQL rendering of privilege statements is pending.

## Related diagnostics

[ownership_role_not_found](ownership_role_not_found.md) concerns the proposed
owner in an ALTER OWNER statement. [query_deparse](query_deparse.md) concerns
SQL rendering.
