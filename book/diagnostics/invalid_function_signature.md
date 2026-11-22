# invalid_function_signature

A PostgreSQL function signature could not be registered because its return or
parameter types could not be bound. The diagnostic identifies the function and
explains the invalid or missing type or schema. Parameter types must already
resolve, and a shell return type cannot have a type modifier.

Correct the type or schema name, or declare the missing parameter type before
the function. An invalid signature leaves the catalog unchanged.
