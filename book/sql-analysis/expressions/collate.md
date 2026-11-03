# COLLATE

## Overview

PostgreSQL `COLLATE` names are resolved against the loaded collation catalog,
using schema qualification or the configured search path. Missing names are
diagnosed. An annotation preserves its operand's type, nullability and output
name; an explicit column alias still takes precedence. Known PostgreSQL operand
types that do not support collation are diagnosed when their type is available.

Analysis retains nested annotations and unresolved names. It does not yet derive
collations across expressions or check precedence and collation conflicts among
function/operator inputs, query boundaries or ordering expressions.

## Dialect and engine-specific behavior

PostgreSQL expression lookup uses the loaded collation catalog and configured
search path. Unknown literals do not by themselves prove that an operand is
non-collatable. The [type and cast rules](types-and-casts.md) apply
independently.

SQLite explicit expression COLLATE uses the analysis context's expression
collation resolver. The supplied contexts do not register names through that
resolver, so even BINARY, NOCASE and RTRIM currently produce a not-found
diagnostic. This applies to projections SQLite itself could execute without
using the collation. This is a limit of sql-fun expression analysis, not
evidence that these names are invalid SQLite SQL.

[INSERT conflict targets](../statements/insert.md) use a separate collation
lookup that knows SQLite's built-in names and explicitly declared custom names.
Registering an INSERT conflict collation does not enable expression COLLATE.
Declaration COLLATE (table columns, domains, composite attributes and record
function definitions) also has a separate validation lifecycle.

## Diagnostics

- [collation_not_found]: the requested expression collation is missing from its
applicable resolver.
- [collation_name_ambiguous]: PostgreSQL lookup finds multiple eligible
  collations.
- [collate_operand_type]: a known PostgreSQL operand type does not support
  collation.

[collation_not_found]: ../../diagnostics/collation_not_found.md
[collation_name_ambiguous]: ../../diagnostics/collation_name_ambiguous.md
[collate_operand_type]: ../../diagnostics/collate_operand_type.md

[collate_capability_undefined](../../diagnostics/collate_capability_undefined.md)
is a declaration-validation diagnostic; expression COLLATE does not emit it
merely because capability metadata is unknown.
