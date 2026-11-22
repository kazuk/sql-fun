# CREATE ACCESS METHOD

sql-fun analyzes PostgreSQL `CREATE ACCESS METHOD` declarations for both
`TABLE` and `INDEX` methods. Schema and extension scripts register successful
declarations for later statements. Access-method names are database-wide.
A duplicate name, including one already used by a builtin or a method of the
other kind, reports a diagnostic and preserves the original registration.

sql-fun retains the handler name as written, including any qualification.
PostgreSQL resolves the handler and checks its signature when you execute
the declaration. PostgreSQL calls the native handler when the method is used.

If you use a builtin catalog extracted before access-method declarations
were included, regenerate it to make builtin methods available to lookup.
