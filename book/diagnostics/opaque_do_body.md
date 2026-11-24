# opaque_do_body

sql-fun does not analyze or execute the procedural body of a PostgreSQL
`DO` statement. Analysis continues with the catalog as it stood before
the block. Any catalog effects the body would have in PostgreSQL are
unavailable to later statements.

See [DO](../sql-analysis/statements/do.md) for the analysis behavior.
