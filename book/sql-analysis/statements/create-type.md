# CREATE TYPE

## Overview

PostgreSQL enum declarations retain the ordered labels and register a nominal
type for subsequent analysis. Composite declarations retain their named ordered
fields and resolve each field's type, including array identity and requested
modifiers.

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

These are PostgreSQL enum and composite analysis paths, not complete support for
all CREATE TYPE forms. SQLite has no corresponding PostgreSQL named
enum/composite implementation. Defining a type does not install database
input/output functions or validate values at runtime. Unimplemented
base/range-type grammar must not be inferred from enum/composite support.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  enum/composite identity conflicts with an existing object in the relevant
  namespace.
- [column_type_not_found](../../diagnostics/column_type_not_found.md): a
  composite field type cannot be resolved.

Explicit field collations can emit the shared collation lookup/capability
diagnostics.
