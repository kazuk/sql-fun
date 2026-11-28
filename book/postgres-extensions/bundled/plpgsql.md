# plpgsql

The PL/pgSQL language declaration and its handler, inline handler, and
validator.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves. |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

Include the assumed extension installation owner in the datasource's `roles`
list. With the default owner, include `"postgres"`.

```sql
SELECT pg_catalog.plpgsql_validator(NULL::oid);
```

## Analysis boundaries

The bundled script assigns the language to the assumed owner. Initialization
reports `ownership_role_not_found` if the owner is absent from the role list.
Resolving the validator does not validate PL/pgSQL bodies or invoke the
language handlers.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/plpgsql.html)
