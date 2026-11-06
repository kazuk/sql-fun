# Function calls

## Overview

Function analysis resolves the requested name against the selected catalog and
search path, analyzes supplied arguments and selects an applicable overload.
This applies to ordinary positional calls and PostgreSQL named and explicit
`VARIADIC` calls described [below](#postgresql-named-function-arguments).
Arguments are checked even when no applicable function signature can be
selected. Duplicate named arguments and positional arguments following named
arguments are diagnosed. Applicable declarations can include defaulted and
variadic arguments; declared return types determine result typing. Supported
implicit argument conversions participate in determining the call's type.

```sql
SELECT length('abc') AS characters
```

An unresolved parameter can defer overload selection until other occurrences,
casts or explicit hints provide SQL type information. Several surviving input
alternatives do not authorize choosing a client binding. See
[parameter inference](parameters.md) for application finalization and candidate
limits.

Supported PostgreSQL polymorphic families include simple anyelement/anyarray
relationships and compatible-family common types. Compatible inputs are matched
to a common type using their supplied order and implicit conversions. For example,
an `integer` and a `bigint` input can produce a `bigint` result. Parameter-dependent
calls retain possible input and result types until their bindings are established.
Typed arrays and anonymous row values can participate in these calls.
Concrete-versus-polymorphic candidate
ranking remains significant even when both declarations accept an operand. A
structural match alone does not select the overload. Unresolved polymorphic
relationships are deferred where supported; requiring an unbounded catalog
search is an explicit inference limitation, not evidence that the SQL is
invalid.

Mixed numeric compatible-family calls can still fail overload selection. For
example, for `f(a anycompatible, b anycompatible) RETURNS anycompatible`, both
`f(1, 2::bigint)` and `f(b => 2::bigint, a => 1)` currently report
`function_overload_resolution_failed` with an unknown result type, although
PostgreSQL accepts them with a bigint result
([#988](https://github.com/kazuk/sql-fun/issues/988)). This shared inference
limitation also affects ordinary positional calls.

Aggregate FILTER/ordering and OVER modifiers retain and resolve their applicable
expressions. See [WINDOW](../clauses/window.md),
[ORDER BY](../clauses/order-by.md) and [GROUP BY](../clauses/group-by.md) for
scope and legality boundaries. [Table functions](../clauses/from.md) own output
definitions and FROM placement; [conditional expressions](conditional.md) own
special CASE/COALESCE/min-max common-type rules. Analysis does not execute
function bodies or interpret SQL text passed as a string argument.

## PostgreSQL named function arguments

PostgreSQL accepts positional arguments, named arguments (`f(arg => value)` or
`f(arg := value)`), and positional arguments followed by named arguments. For
example, given a SQL function `combine(a text, b text DEFAULT 'B')`,
`combine(a => 'A')` omits `b`, while `combine('A', b => 'C')` supplies both.
Unquoted argument names normalize to lowercase; quoted names retain their case.
Duplicate names and positional arguments following named arguments are diagnosed.

Named calls bind supplied names to input or INOUT parameters of each candidate
signature. Unknown names, double assignment to one parameter, missing required
arguments and unsuitable argument types prevent that candidate from matching.
OUT and TABLE parameters do not consume arguments. Overload ambiguity remains
an error; analysis does not choose an arbitrary candidate. Omitted arguments
remain omissions; defaults establish whether a signature applies, without
inserting or evaluating their expressions.

Explicit `VARIADIC` passes one value at its written position. For a variadic
array parameter, this is the whole array rather than expanded elements. For
example, `f(VARIADIC xs => ARRAY[1, 2])` can supply the declared variadic array
`xs`; naming that parameter without `VARIADIC` does not request array passing.
Empty and NULL arrays can carry explicit array types. PostgreSQL also permits
the keyword on calls to ordinary non-variadic declarations; their declared
argument types still govern matching.

Argument names and written order remain intact during type inference. Parameter
positions are the original placeholders, independently of the order of formal
parameters. Result typing and the existing implicit-conversion and polymorphic
rules apply to these calls, including supported [FROM functions](../clauses/from.md).
Named notation is rejected for aggregate calls without OVER; aggregate window
calls can use it when the declared argument names match.

SQLite does not support
PostgreSQL named-argument notation; its analysis limitations below are separate.

A SQL function argument name identifies a parameter of that SQL function. It is
distinct from [Rust query binding](../../sql_functions/parameter-binding.md):
`${name}` supplies a query input from a Rust argument or DTO.

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

Ordinary SQLite scalar function binding, min/max behavior and general
function-dependent result/parameter inference remain unimplemented and can
panic ([#505](https://github.com/kazuk/sql-fun/issues/505)). The dedicated
COALESCE and LIKE paths are separate implemented subsets with their own
remaining limitations; they do not supply general SQLite function analysis.
SQLite table-valued functions have a separate
[unimplemented source path](../clauses/from.md#sqlite-source-limitations).

## Diagnostics

- [function_not_found]: the selected catalog cannot resolve the requested
function name.
- [function_overload_resolution_failed]: established arguments cannot select an
applicable function signature.
- [function_arg_implicit_cast_not_found]: a selected formal argument requires an
unavailable implicit conversion.
- [call_argument_unknown_name]: no candidate recognizes a supplied name.
- [call_argument_missing_required]: all candidates require the same omitted input.
- [call_argument_assigned_twice]: all candidates assign the same formal both
  positionally and by name.
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

[call_argument_unknown_name]: ../../diagnostics/call_argument_unknown_name.md
[call_argument_missing_required]: ../../diagnostics/call_argument_missing_required.md
[call_argument_assigned_twice]: ../../diagnostics/call_argument_assigned_twice.md
