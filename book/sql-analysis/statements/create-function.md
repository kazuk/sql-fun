# CREATE FUNCTION / CREATE PROCEDURE

## Overview

PostgreSQL routine declarations populate overload metadata for later
[function-call](../expressions/function-calls.md) analysis. Supported
declarations describe parameter names and modes, input/output types, default
expressions, scalar or set-returning results, and explicit strictness. STRICT
and RETURNS NULL ON NULL INPUT mark strict calls; CALLED ON NULL INPUT does not.
OUT/INOUT/TABLE outputs can determine an omitted return declaration; procedures
have no function result type.

```sql
CREATE FUNCTION public.identity_integer(value integer)
RETURNS integer LANGUAGE sql STRICT AS 'SELECT value';
```

Routine signatures participate in their own overload namespace and supported OR
REPLACE behavior. Recognized SQL bodies and language/options are not
automatically validated or executed during declaration analysis. A declared
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

### Custom aggregates

`CREATE AGGREGATE` is legal PostgreSQL syntax but is not converted into an
analyzable top-level declaration. A schema file containing it fails initialization
with `unexpected input :top-level PostgreSQL statement`
([#987](https://github.com/kazuk/sql-fun/issues/987)). This prevents loading custom
aggregate definitions from that file; it does not prevent using aggregate
metadata already present in the selected builtin catalog.

### Declaration validation gaps

Routine declaration analysis is incomplete
([#640](https://github.com/kazuk/sql-fun/issues/640)). It retains overload
metadata but does not validate SQL body references/statements or all retained
language and routine options. Constructing that metadata does not establish
complete agreement between explicit return declarations and output-parameter
result shapes. Lack of sql-fun body validation does not determine when
PostgreSQL validates a language-specific body.

Unknown parameter type references and missing scalar return types can remain
unresolved in overload metadata without the required missing-type diagnostic
([#566](https://github.com/kazuk/sql-fun/issues/566)). A missing set-returning
result type instead terminates analysis as described below.
A returned declaration is therefore not proof that all declared type references
and explicit-versus-derived outputs were validated. The OUT/INOUT/TABLE and
procedure metadata support described above remains distinct from these gaps.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): a routine
  signature conflicts with an existing routine identity or replacement kind.

Defaults use shared expression diagnostics. A missing set-returning result type
terminates analysis; it is not currently a
successful declaration or an emitted function-lookup diagnostic. Declaration
analysis is not a complete body validator.
