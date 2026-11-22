# CREATE OPERATOR CLASS

sql-fun recognizes PostgreSQL `CREATE OPERATOR CLASS` declarations. Its
analysis server panics and exits when analysis reaches one in SQL input or
in a schema or extension script. The CLI reports `EOF with missing responses`,
and the declaration is never registered.

This limitation affects bundled extensions such as `btree_gin`. Follow
[issue #1220](https://github.com/kazuk/sql-fun/issues/1220) for operator-class
analysis and [issue #1150](https://github.com/kazuk/sql-fun/issues/1150) for
extension initialization.

The PostgreSQL 18 parser rejects the legacy `RECHECK` clause even when
analysis uses a PostgreSQL 17 catalog. Remove the clause from older scripts
before analysis. PostgreSQL 17 accepts it as a no-op.
[Issue #1221](https://github.com/kazuk/sql-fun/issues/1221) tracks parser
compatibility for legacy dumps.
