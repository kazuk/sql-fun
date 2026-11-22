# CREATE OPERATOR CLASS

sql-fun analyzes PostgreSQL operator-class declarations in schema and extension
scripts. The `USING` name must identify an existing index access method. Builtin
methods and methods declared earlier in the script are available. The input
type, referenced operators, and support routines must exist before the
declaration.

An explicit `FAMILY` names an existing family for the same access method. When
`FAMILY` is omitted, the class uses a family with the same name and schema,
creating the family if needed. A `DEFAULT` class must be unique for its input
type and access method. A rejected declaration leaves the existing family and
classes unchanged.

Later [INSERT](insert.md) statements can resolve explicit conflict-target
operator classes. [CREATE INDEX](create-index.md) has its own analysis limits.
sql-fun resolves the support routine by its SQL signature. PostgreSQL checks
whether that routine satisfies the native access method’s calling contract.

The PostgreSQL 18 parser rejects the legacy `RECHECK` clause even when analysis
uses a PostgreSQL 17 catalog. Remove the clause from older scripts before
analysis. PostgreSQL 17 accepts it as a no-op.
[Issue #1221](https://github.com/kazuk/sql-fun/issues/1221) tracks parser
compatibility for legacy dumps.
