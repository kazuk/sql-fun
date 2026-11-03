# HAVING

## Overview

HAVING analyzes supported expressions in the grouped query's source scope.
PostgreSQL resolves source columns rather than SELECT output aliases; an alias
accepted as a standalone ORDER BY item is not therefore a HAVING binding.
Predicate parameter inputs participate in the statement's constraints and require
PostgreSQL boolean-compatible types.

```sql
SELECT a FROM (SELECT 1 AS a) AS source
GROUP BY a HAVING GROUPING(a) IS NOT NULL;
```

See [GROUP BY](group-by.md) for GROUPING key ownership and
[predicates](../expressions/operators-and-predicates.md) for NULL/operator
semantics. Analysis does not calculate groups or execute the filter.

## Dialect and engine-specific behavior

SQLite makes applicable output aliases available to its HAVING expression lookup
and does not apply PostgreSQL boolean constraints. Its full grouping/HAVING
legality and type-affinity checks remain incomplete. PostgreSQL aggregate/window
legality is also not completely validated merely because names and GROUPING
arguments resolve.

Parameter-free PostgreSQL HAVING operands share the
[boolean-validation gap](where.md#parameter-free-boolean-validation) tracked in
[#575](https://github.com/kazuk/sql-fun/issues/575).

## Diagnostics

[Name-resolution diagnostics](../expressions/names.md#diagnostics) report missing
or ambiguous references, including a PostgreSQL HAVING reference that only names
an output alias. Conflicting parameter uses can report
[parameter_constraint](../../diagnostics/parameter_constraint.md).
[GROUPING diagnostics](group-by.md#diagnostics) apply when a HAVING expression
uses that operation. Other expression diagnostics remain applicable.
