# base_type_invalid_definition

A PostgreSQL base type declaration could not be applied. The diagnostic names
the type and explains the invalid option, missing shell, or support function
that prevented completion.

Declare the shell and its support functions before the full definition. Match
each support function's required argument and return types. For fixed-length
types, use `STORAGE = plain`.

Analysis continues with the catalog as it was before the rejected statement.
An existing shell remains available to function declarations and can be
completed by a later valid definition.
