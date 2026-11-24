# DO

sql-fun parses PostgreSQL `DO` statements, including both positions of the
optional `LANGUAGE` clause. Schema and extension analysis stops when it reaches
a `DO` block. The block is neither interpreted nor executed, so any catalog
changes it would make in PostgreSQL are unavailable during analysis.

[Issue #1250](https://github.com/kazuk/sql-fun/issues/1250) tracks semantic
support.
