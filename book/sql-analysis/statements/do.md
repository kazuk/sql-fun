# DO

sql-fun accepts PostgreSQL `DO` statements with or without an optional
`LANGUAGE` clause. The procedural body is opaque. sql-fun does not analyze or
execute its contents, and potential side effects are not reflected in the
analysis catalog. Each block emits the
[opaque_do_body](../../diagnostics/opaque_do_body.md) warning at the `DO` keyword.

Schema and extension analysis continue with the catalog as it stood before
the block. If a later statement refers to a table created only inside the
block, sql-fun reports that the table is missing.

See [issue #1250](https://github.com/kazuk/sql-fun/issues/1250) for the analysis
boundary.
