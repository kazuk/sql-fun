# invalid_language_declaration

Schema analysis could not apply a PostgreSQL `CREATE LANGUAGE` declaration.
Check that each named routine is visible and has the exact input signature
required for its clause. The call handler must also return
`pg_catalog.language_handler`. If the language already exists, use
`CREATE OR REPLACE LANGUAGE` to replace its definition intentionally.

[CREATE LANGUAGE reference](../sql-analysis/statements/create-language.md)
