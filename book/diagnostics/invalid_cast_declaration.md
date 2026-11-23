# invalid_cast_declaration

Schema analysis could not apply a PostgreSQL `CREATE CAST` declaration.
Check that both types and any referenced function are already defined, that the
function or binary layout matches the specified types, and that the directed
cast has not already been declared.
