# base_type_invalid_definition

Analysis rejected a PostgreSQL base type declaration or settings change. The
diagnostic names the type and explains the missing prerequisite, invalid
option, or invalid support function.

Declare the shell and its support functions before the full definition. Match
each support function's required argument and return types. If the declaration
uses `LIKE`, make sure its source type is already defined and visible in the
search path, or qualify its name with the schema. For `ALTER TYPE` storage
changes, check whether the type's layout and current storage strategy permit
the requested change.
[Base type settings changes](../sql-analysis/statements/create-type.md#postgresql-base-type-settings-changes)

Analysis continues with the catalog as it was before the rejected statement.
An existing shell remains available to function declarations and can be
completed by a later valid definition.
