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

### Builtin array type names

Function signatures can use PostgreSQL's builtin array catalog names. For
example, `_text` and `pg_catalog._text` denote the same type as `text[]`
when they resolve to the array in `pg_catalog`. Unqualified names follow the
configured search path. This lookup requires builtin catalog data that includes
the array names, such as the updated bundled catalogs. Bracket notation
continues to work with older catalog data.

### Implicit shell types

For PostgreSQL schema and extension analysis, a `LANGUAGE C` or
`LANGUAGE internal` function can introduce a missing return type as a shell.
The shell is registered with the function. A qualified return name selects its
schema. An unqualified name uses the current creation schema, even if the
function is declared in another schema. An existing scalar shell is reused.
The parameter types must already resolve when the function is declared.

On PostgreSQL 15–18, this also applies to `SETOF` and unresolved array return
declarations such as `RETURNS missing_type[]`. The array fallback registers a
shell named `missing_type`, and the function's return type refers to that
shell. An existing array type resolves normally. An existing shell with the
same name in the creation schema conflicts with the array fallback.

A later [CREATE TYPE](create-type.md) can complete the shell used by the
type's input and output routines. Failed function declarations in this path
leave existing catalog entries intact.

### Custom aggregates

Schema analysis can register ordinary PostgreSQL `CREATE AGGREGATE`
declarations with fixed input arguments and the options used by
`named_sum`, `citext` and `intagg`. The aggregate's result type comes
from `STYPE` or from `FINALFUNC` when supplied.

Schema analysis stops at aggregate declarations whose argument form or options
are outside this supported subset. See
[#987](https://github.com/kazuk/sql-fun/issues/987) for the ordinary aggregate
implementation.
Semantic support for custom ordered-set and hypothetical-set aggregates, including
`WITHIN GROUP` call analysis, is tracked in
[#1256](https://github.com/kazuk/sql-fun/issues/1256).

### Declaration validation gaps

Routine declaration analysis is incomplete
([#640](https://github.com/kazuk/sql-fun/issues/640)). It retains overload
metadata but does not validate SQL body references/statements or all retained
language and routine options. Constructing that metadata does not establish
complete agreement between explicit return declarations and output-parameter
result shapes. Lack of sql-fun body validation does not determine when
PostgreSQL validates a language-specific body.

Unknown parameter type references and some missing scalar return types can
remain unresolved in overload metadata without the required missing-type
diagnostic ([#566](https://github.com/kazuk/sql-fun/issues/566)). An unresolved
set-returning result type rejects the declaration.
A returned declaration is therefore not proof that all declared type references
and explicit-versus-derived outputs were validated. The OUT/INOUT/TABLE and
procedure metadata support described above remains distinct from these gaps.

## Diagnostics

- [invalid_function_signature](../../diagnostics/invalid_function_signature.md)
  reports a rejected routine signature during implicit shell analysis.
- [conflicting_creation](../../diagnostics/conflicting_creation.md): a routine
  signature conflicts with an existing routine identity or replacement kind.

Defaults use shared expression diagnostics. A missing set-returning result type
reports `invalid_function_signature` when the declaration cannot create a shell.

## ALTER FUNCTION PARALLEL

sql-fun can convert and render PostgreSQL `ALTER FUNCTION` statements with a
single `PARALLEL SAFE`, `PARALLEL RESTRICTED`, or `PARALLEL UNSAFE` action. It
preserves the function name and signature, including the distinction between
an omitted signature and explicit empty parentheses. Statements that combine
`PARALLEL` with other actions are unsupported in this conversion.

Schema and extension analysis records the requested mode on the selected
function. An explicit signature matches exact input argument types, including
`INOUT` and `VARIADIC` arguments. Argument names and `OUT`-only arguments do
not participate in matching. Empty parentheses select a zero-input function.
Without a signature, the function name must identify a single visible candidate.
An unqualified name uses the search path, where an earlier schema hides a
later candidate with the same input signature. A schema-qualified name
restricts lookup to that schema. Ordinary and window functions can be altered.

Missing and ambiguous targets report
[`ownership_target_not_found`](../../diagnostics/ownership_target_not_found.md)
and [`ownership_target_ambiguous`](../../diagnostics/ownership_target_ambiguous.md).
An unknown input type reports
[`ownership_argument_type_not_found`](../../diagnostics/ownership_argument_type_not_found.md).
An explicitly matched procedure or a selected aggregate reports
[`ownership_invalid_target_kind`](../../diagnostics/ownership_invalid_target_kind.md).
Analysis continues after a rejected change using the existing definition.
