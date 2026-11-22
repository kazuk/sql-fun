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
A shell cannot serve as a table column type. Base definitions check the
declared internal length, alignment, storage and pass-by-value combination
before the completed type becomes available. sql-fun requires `CATEGORY` to
contain exactly one printable ASCII character. PostgreSQL 15–18 use the first
character when a longer value is supplied. Definitions using `LIKE` stop
analysis because inherited layout validation is pending
([#1211](https://github.com/kazuk/sql-fun/issues/1211)).

Generated array catalog names such as `_name` are not registered for
user-defined base types ([#1209](https://github.com/kazuk/sql-fun/issues/1209)).
Declare the shell explicitly before its functions. Implicit shell creation in
C-language function declarations remains unsupported
([#1210](https://github.com/kazuk/sql-fun/issues/1210)). Range type declarations
remain unsupported.

PostgreSQL ALTER TYPE operations that alter composite types remain unimplemented
and can terminate analysis
([#543](https://github.com/kazuk/sql-fun/issues/543)). CREATE TYPE metadata does
not establish ALTER TYPE catalog transitions. SQLite/Turso CREATE/DROP TYPE
operations are separately listed in the [statement support
inventory](../statements.md#unimplemented-sqlite-and-turso-statement-analysis).

Enum casts currently resolve the enum type without validating literal label
text: for example, `'unknown'::public.item_state` does not receive a diagnostic
for an absent label. Individual enum-label references are not yet available
for deprecated-label linting
([#1084](https://github.com/kazuk/sql-fun/issues/1084)).

## Diagnostics
- [base_type_invalid_definition](../../diagnostics/base_type_invalid_definition.md) reports a rejected base definition. An existing shell remains available for a later valid completion.

- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  enum/composite identity conflicts with an existing object in the relevant
  namespace.
- [column_type_not_found](../../diagnostics/column_type_not_found.md): a
  composite field type cannot be resolved.

Explicit field collations can emit the shared collation lookup/capability
diagnostics.
