# CREATE CAST

sql-fun parses PostgreSQL `CREATE CAST` declarations, including
`WITH FUNCTION`, `WITHOUT FUNCTION`, and `WITH INOUT`. Schema analysis
currently stops at these declarations. Resolving the declared types and
function and making the cast available to later SQL are tracked in
[#1237](https://github.com/kazuk/sql-fun/issues/1237).
