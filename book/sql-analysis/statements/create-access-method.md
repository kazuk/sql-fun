# CREATE ACCESS METHOD

sql-fun recognizes PostgreSQL `CREATE ACCESS METHOD` declarations for both
`TABLE` and `INDEX` access methods. When analysis reaches a declaration in SQL
input or a schema or extension script, the analysis server exits and the CLI
reports `EOF with missing responses`.

This limitation affects initialization with the bundled `bloom` extension.
Follow [issue #1223](https://github.com/kazuk/sql-fun/issues/1223) for access
method registration and [issue #1150](https://github.com/kazuk/sql-fun/issues/1150)
for bundled extension initialization.
