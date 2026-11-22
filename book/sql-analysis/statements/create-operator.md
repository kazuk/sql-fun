# CREATE OPERATOR

sql-fun accepts PostgreSQL `CREATE OPERATOR` syntax during parsing. Semantic
analysis is unimplemented and stops at the declaration.
The declaration does not add an operator to the analysis catalog for use by
later statements. This also affects schema initialization from extension scripts
that declare operators, including the bundled `pg_trgm` installation script.
Follow [issue #1215](https://github.com/kazuk/sql-fun/issues/1215) for operator
analysis and [issue #1150](https://github.com/kazuk/sql-fun/issues/1150) for
extension initialization.
