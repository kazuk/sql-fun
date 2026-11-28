# foreign_data_wrapper_invalid_definition

Code: `foreign_data_wrapper_invalid_definition`. Severity: Error.

sql-fun reports this diagnostic when `CREATE FOREIGN DATA WRAPPER` repeats
a wrapper name, a handler or validator clause, or an option name, or when
it cannot resolve a routine with the required signature. A database-qualified
routine must refer to the local database. A handler must
take no arguments and return `fdw_handler`. A validator must take
`text[], oid`.

Choose a unique wrapper name in the database, specify each clause and option
once, and check the routine declaration and active search path. Use a
schema-qualified routine name to select a particular schema.

See [PostgreSQL utility statements](../sql-analysis/statements.md#other-postgresql-utility-statements).
