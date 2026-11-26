# CREATE TYPE

## Overview

PostgreSQL enum declarations define a named type with labels in written order
for subsequent analysis. Composite declarations define named fields in written
order and check each field's type, including arrays and declaration modifiers.

```sql
CREATE TYPE public.item_state AS ENUM ('open', 'closed');
CREATE TYPE public.item_key AS (id integer, label text);
```

Composite field collations use the shared [declaration
COLLATE](../expressions/collate.md) validation. Failed field/type/collation
validation does not authorize publishing a usable composite definition.
Standalone composites participate in PostgreSQL relation/type namespace
conflicts; see the [creation policy](../statements.md).

## Dialect and engine-specific behavior

PostgreSQL analysis supports enum and composite declarations. SQLite has no
corresponding named enum or composite implementation. Defining a type does
not install database input/output functions or validate values at runtime.

In PostgreSQL schema and extension analysis, a shell declaration
(`CREATE TYPE name`) reserves the type name so its input and output functions
can declare it as an argument or result. A subsequent base definition
(`CREATE TYPE name (...)`) resolves its support functions and makes the
completed type and its `name[]` array form available to subsequent analysis.
A shell cannot serve as a table column type. Base definitions retain the
written physical-layout options in order.
A `LIKE` clause resolves its source type using PostgreSQL type names and
the active search path. PostgreSQL validates the physical layout at execution.

Generated array catalog names such as `_name` are not registered for
user-defined base types ([#1209](https://github.com/kazuk/sql-fun/issues/1209)).
An input routine can also introduce the shell through
[CREATE FUNCTION](create-function.md). The completed type keeps the identity
used by the support functions. Range type declarations remain unsupported.

SQLite/Turso CREATE/DROP TYPE
operations are separately listed in the [statement support
inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).

Enum casts currently resolve the enum type without validating literal label
text: for example, `'unknown'::public.item_state` does not receive a diagnostic
for an absent label. Individual enum-label references are not yet available
for deprecated-label linting
([#1084](https://github.com/kazuk/sql-fun/issues/1084)).

## PostgreSQL base type settings changes

Schema and extension analysis supports `ALTER TYPE name SET (...)` on
completed PostgreSQL base types. A schema-qualified name selects the type
directly, and an unqualified name resolves through the active search path.

`RECEIVE`, `SEND`, `TYPMOD_IN`, `TYPMOD_OUT`, `ANALYZE`, and
`SUBSCRIPT` accept a support function with the required argument and return
types, or `NONE` to remove the function in PostgreSQL. Analysis validates
these settings without invoking the functions. `STORAGE` accepts `plain`,
`external`, `extended`, or `main`. Subsequent statements use the updated
storage setting. Fixed-size types require `plain`, and a type with a
non-plain setting cannot return to `plain`. If a statement repeats
`STORAGE`, each assignment is checked against the setting before the
statement and the last value takes effect. A rejected statement preserves
the type and its previous settings.

A standalone `analyze` or `test` input can report the semantic result
alongside `query_deparse` while executable SQL rendering is pending
([#1262](https://github.com/kazuk/sql-fun/issues/1262)).

Some builtin type support functions, such as `pg_catalog.varcharrecv` and
`pg_catalog.jsonb_subscript_handler`, are missing from the routine catalog.
Naming one in an alteration produces a support-function diagnostic
([#1260](https://github.com/kazuk/sql-fun/issues/1260)).

A catalog extracted before base type eligibility and storage metadata were
available may need a fresh extraction to analyze these alterations. PostgreSQL
enforces ownership and superuser requirements when executing the statement.

PostgreSQL ALTER TYPE operations that alter composite types remain unimplemented
and can terminate analysis
([#543](https://github.com/kazuk/sql-fun/issues/543)).

## Diagnostics

- [base_type_invalid_definition](../../diagnostics/base_type_invalid_definition.md)
  reports a rejected base definition or settings change. An existing shell
  remains available for
  a later valid completion.
- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  enum/composite identity conflicts with an existing object in the relevant
  namespace.
- [column_type_not_found](../../diagnostics/column_type_not_found.md): a
  composite field type cannot be resolved.

Explicit field collations can emit the shared collation lookup/capability
diagnostics.
