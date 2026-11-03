# Type resolution and casts

## Overview

An explicit SQL cast establishes the result type of its operand. PostgreSQL
resolves named target types using the explicitly selected catalog and search
path, then checks supported explicit conversions. Implicit conversions used by
operators, functions, common-type expressions and assignments follow their own
applicability rules; an available explicit cast does not make it implicit.

```sql
SELECT CAST(1 AS bigint) AS widened, ARRAY[]::integer[] AS empty_values
```

A result cast on an unresolved parameter can admit several source types. It is
not an explicit client input type hint; see [parameters](parameters.md).
Anonymous rows can be coerced positionally to compatible named row targets,
without requiring an ordinary named cast entry for the anonymous row. Array
casts retain array identity and can supply an element target to empty or nested
[ARRAY constructors](arrays-and-rows.md).

[CASE, COALESCE, GREATEST and LEAST](conditional.md) and ARRAY list constructors
select common types using PostgreSQL categories, preferred-type facts and
implicit conversions. Unknown SQL literals and unresolved client inputs are
different: all-unknown literals resolve to text in these common-type operations,
while wholly unresolved input-dependent results can retain open domains.

### Declaration type modifiers

Declaration modifiers are separate from nominal type identity. Record-returning
[FROM function definitions](../clauses/from.md#record-returning-functions) own
the builtin length, numeric precision/scale and time/interval rules, including
invalid arguments, clamping and unvalidated extension modifiers. Declaration
modifiers are not accepted in
[client input hints](parameters.md#sqlast-api-callers).

## Dialect and engine-specific behavior

SQLite CAST interprets the declared name using SQLite affinity rules, without
requiring a PostgreSQL cast declaration. This does not provide complete SQLite
function/operator type-affinity analysis or runtime constraint validation.
PostgreSQL named types, domains and catalog conversions are not silently applied
to SQLite expressions. Names must be resolved in the configured analysis
context; analysis does not query a live database for missing types.

## Diagnostics

- [explicit_cast_not_defined]: a known PostgreSQL source and target lack the
required explicit conversion.
- [implicit_cast_not_found]: a known expression cannot satisfy an implicit
target conversion.
- [parameter_constraint]: an unresolved cast target or incompatible joint
parameter requirements prevent analysis.
- [table_function_invalid_typmod]: a builtin modifier in a table-function output
declaration is invalid.

[explicit_cast_not_defined]: ../../diagnostics/explicit_cast_not_defined.md
[implicit_cast_not_found]: ../../diagnostics/implicit_cast_not_found.md
[parameter_constraint]: ../../diagnostics/parameter_constraint.md
[table_function_invalid_typmod]: ../../diagnostics/table_function_invalid_typmod.md

Declaration owners also report missing type names, such as
[column_type_not_found](../../diagnostics/column_type_not_found.md). This code
does not replace the expression cast-target diagnostic above.
