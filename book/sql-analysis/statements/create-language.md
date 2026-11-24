# CREATE LANGUAGE

For PostgreSQL schema SQL, sql-fun analyzes language declarations that specify
`HANDLER`. The language name is database-wide. Unqualified routine names
follow the search path, and each routine must match the input signature
required by its clause.

| Clause | Input arguments | Required return type |
| --- | --- | --- |
| `HANDLER` | None | `pg_catalog.language_handler` |
| `INLINE` | `pg_catalog.internal` | Any |
| `VALIDATOR` | `pg_catalog.oid` | Any |

`INLINE` and `VALIDATOR` are optional. Routine lookup uses the declared input
types, without applying implicit casts or filling in default arguments.

A successful declaration registers the language for later schema statements.
A second `CREATE LANGUAGE` for the same name reports an error.
`CREATE OR REPLACE LANGUAGE` can create a new entry or retain an existing
language's identity and ownership while accepting a new declaration.
Invalid signatures, missing routines, and duplicates leave the analysis
catalog unchanged. PostgreSQL enforces privileges and invokes the routines
when needed.

See [invalid_language_declaration](../../diagnostics/invalid_language_declaration.md)
for declaration errors.
