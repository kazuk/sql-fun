# CREATE FUNCTION / CREATE PROCEDURE

## Overview

PostgreSQL routine declarations populate overload metadata for later
[function-call](../expressions/function-calls.md) analysis. The implemented
paths retain parameter names and modes, input/output types, default expressions,
scalar or set-returning result metadata, and explicit strictness. STRICT and
RETURNS NULL ON NULL INPUT mark strict calls; CALLED ON NULL INPUT does not.
OUT/INOUT/TABLE outputs can determine an omitted return declaration; procedures
have no function result type.

```sql
CREATE FUNCTION public.identity_integer(value integer)
RETURNS integer LANGUAGE sql STRICT AS 'SELECT value';
```

Routine signatures participate in their own overload namespace and supported OR
REPLACE behavior. SQL bodies and language/options retained by parsing are not
automatically inspected or executed by this declaration analyzer. A declared
trigger function's metadata can be used by [CREATE TRIGGER](triggers.md); body
text is not evidence of trigger correctness.

## Dialect and engine-specific behavior

This is PostgreSQL routine-catalog analysis, including supported procedure
declaration metadata. SQLite registered runtime functions and affinity are a
separate contract; PostgreSQL CREATE FUNCTION support does not install or
analyze a SQLite extension function. Concrete signatures are usable where
implemented; unresolved polymorphic call signatures retain the limitations
described at the function-call page. Missing result/parameter facts do not prove
that every valid routine declaration is rejected or fully checked.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): a routine
  signature conflicts with an existing routine identity or replacement kind.

Defaults use shared expression diagnostics. A missing set-returning result type
reaches an explicitly unimplemented reporting helper; it is not currently a
successful declaration or an emitted function-lookup diagnostic. Declaration
analysis is not a complete body validator.
