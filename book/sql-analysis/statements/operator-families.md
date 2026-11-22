# CREATE OPERATOR FAMILY and ALTER OPERATOR FAMILY

`CREATE OPERATOR FAMILY` declares a PostgreSQL operator family for an existing
index access method. Families can share a schema and name if their methods
differ. A later [CREATE OPERATOR CLASS](create-operator-class.md) can reference
the family by a qualified name or through the search path.

`ALTER OPERATOR FAMILY … ADD` and `… DROP` update independently added operator
and support-function memberships. A drop specifies the strategy or support
number and its associated type pair. PostgreSQL can bind members to a class or require them for
the family. Those members and builtin members cannot be removed this way. Dropping an independent membership leaves the operator or
routine itself available. If any requested change fails, the entire statement
leaves the family unchanged.

See PostgreSQL's [ALTER OPERATOR FAMILY documentation](https://www.postgresql.org/docs/18/sql-alteropfamily.html)
for additional forms.
