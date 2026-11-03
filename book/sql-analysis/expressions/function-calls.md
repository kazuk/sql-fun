# Function calls

## Overview

Function analysis resolves the requested name against the selected catalog and
search path, analyzes supplied arguments and selects an applicable overload.
Argument positions and named arguments are retained independently of whether an
overload can be selected. Duplicate named arguments and positional arguments
following named arguments are diagnosed. Applicable declarations can include
defaulted and variadic arguments; declared return types determine result typing.
Supported implicit argument conversions are retained in the analyzed call.

```sql
SELECT length('abc') AS characters
```

An unresolved parameter can defer overload selection until other occurrences,
casts or explicit hints provide SQL type information. Several surviving input
alternatives do not authorize choosing a client binding. See
[parameter inference](parameters.md) for application finalization and candidate
limits.

Supported PostgreSQL polymorphic families include simple anyelement/anyarray
relationships and compatible-family common types. Typed arrays and anonymous
rows can participate without inventing catalog names for structural values.
Concrete-versus-polymorphic candidate ranking remains significant even when both
declarations accept an operand. A structural match alone does not select the
overload. Unresolved polymorphic relationships are deferred where supported;
requiring an unbounded catalog search is an explicit inference limitation, not
evidence that the SQL is invalid.

Aggregate FILTER/ordering and OVER modifiers retain and resolve their applicable
expressions. See [WINDOW](../clauses/window.md),
[ORDER BY](../clauses/order-by.md) and [GROUP BY](../clauses/group-by.md) for
scope and legality boundaries. [Table functions](../clauses/from.md) own output
definitions and FROM placement; [conditional expressions](conditional.md) own
special CASE/COALESCE/min-max common-type rules. Analysis does not execute
function bodies or interpret SQL text passed as a string argument.

## Dialect and engine-specific behavior

PostgreSQL overload selection depends on the supplied declarations and cast
metadata; install the selected builtin and extension catalogs explicitly.
Range/multirange construction and subtype metadata remain incomplete
([#914](https://github.com/kazuk/sql-fun/issues/914)); supported array/row
polymorphism does not establish those operations. Function name resolution and
modifier binding do not establish complete aggregate/window legality checking.

SQLite function type affinity and general overload-dependent inference remain
incomplete. PostgreSQL overload and implicit-cast behavior must not be
advertised as SQLite analysis behavior. Special SQLite LIKE and COALESCE
semantics are documented on their [predicate](operators-and-predicates.md) and
[conditional](conditional.md) pages.

## Diagnostics

- [function_not_found]: the selected catalog cannot resolve the requested
function name.
- [function_overload_resolution_failed]: established arguments cannot select an
applicable function signature.
- [function_arg_implicit_cast_not_found]: a selected formal argument requires an
unavailable implicit conversion.
- [call_argument_duplicate_name]: a named argument is supplied more than once.
- [call_argument_positional_after_named]: a positional argument follows a named
  argument.

[function_not_found]: ../../diagnostics/function_not_found.md
[function_overload_resolution_failed]: ../../diagnostics/function_overload_resolution_failed.md
[function_arg_implicit_cast_not_found]: ../../diagnostics/function_arg_implicit_cast_not_found.md
[call_argument_duplicate_name]: ../../diagnostics/call_argument_duplicate_name.md
[call_argument_positional_after_named]: ../../diagnostics/call_argument_positional_after_named.md

[Parameter diagnostics](parameters.md#diagnostics) also apply to deferred calls.
Clause pages document diagnostics from FILTER/ordering/window expressions.
