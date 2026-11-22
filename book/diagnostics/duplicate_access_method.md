# duplicate_access_method

Code: `duplicate_access_method`. Severity: Error.

A `CREATE ACCESS METHOD` declaration uses a name that already identifies
an access method in the schema catalog. Builtins, table access methods,
and index access methods share the same namespace.

Choose a unique name for the new method. The existing registration remains
available to later statements.
