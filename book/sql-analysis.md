# SQL analysis coverage and limitations

This page is for application developers writing queries or investigating SQL
analysis diagnostics. It describes supported forms and current limitations;
installation and initial configuration are covered in [Installation](installation.md).

## SQLite parser limitations

SQLite support is under development. Its sql-fun parser currently limits
expression depth to 100. Consequently, some SQL accepted by SQLite itself may be
rejected during sql-fun parsing. This limit applies to expression trees, including
long chains of operators, not just nested parentheses. There is currently no
sql-fun setting to increase it.

UPDATE and DELETE statements with `ORDER BY` or `LIMIT` clauses are also rejected
by the inherited parser. SQLite itself supports those clauses only in builds
with `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`; enabling that option in the database
does not enable the grammar in sql-fun. There is no sql-fun setting or
preprocessing workaround for these clauses.

The parser also recognizes Turso extensions. Successful syntax parsing does not
imply that stock SQLite or `rusqlite` can execute those extensions.

## SQLite analysis coverage

With the `sqlite` feature enabled, SELECT analysis supports literal projections,
column references from derived queries and nonrecursive CTEs, join nullability,
compound queries, and CASE expressions whose result branches have the same type.
Use explicit `AS` names when stable result-column names matter.

This is not complete SQLite query validation. Arithmetic and comparison operators,
function type affinity, case-insensitive identifier binding,
operator-dependent parameter analysis, star expansion, recursive CTE binding,
and full compound type checking remain incomplete.
Supported expressions in GROUP BY, ORDER BY, HAVING, LIMIT/OFFSET, and window
clauses participate in analysis. Output aliases and ordering positions follow
SQLite binding rules; compound ordering can refer to either operand's output
expressions or aliases. Full grouping and window legality checks remain incomplete.
Different CASE result types are not unified using PostgreSQL implicit casts.
Use matching identifier spelling when referring to derived-query or CTE columns.

## Parameter analysis

Numbered parameters (`$1` in PostgreSQL and `?1` in SQLite) can be used across
supported nonrecursive CTEs and the outer query. Repeated uses of a parameter
must have compatible SQL type requirements; conflicts produce diagnostics.
A parameter used only by `IS NULL` has no particular type requirement. Explicit
SQL casts constrain accepted input types. SQLite casts follow SQLite semantics
and do not require PostgreSQL cast rules.

PostgreSQL analysis accounts for implicit casts in operator and ordinary function
calls. A parameter or a parameter-dependent result column may have several
admissible types.

Parameters in supported ORDER BY, GROUP BY, HAVING, DISTINCT ON, LIMIT/OFFSET,
aggregate FILTER/ordering, and window expressions participate in analysis,
including nested queries. Parameter-dependent RANGE ordering and custom RANGE
operator classes are not fully analyzed.
PostgreSQL output aliases are available as standalone ORDER BY/GROUP BY items;
GROUP BY gives source columns precedence, and HAVING uses source names.
Missing and ambiguous references produce diagnostics. Successful name binding
does not imply complete aggregate or window legality checking.

Concrete PostgreSQL function signatures are supported; unresolved polymorphic
signatures remain unsupported. Type unification of unresolved inputs in CASE,
COALESCE, and indirection is unimplemented and can terminate analysis.
