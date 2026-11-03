# CREATE EXTENSION

## Overview

The PostgreSQL schema analyzer records the requested extension name and
supported IF NOT EXISTS behavior. A duplicate without that explicit flag reports
a creation conflict.

```sql
CREATE EXTENSION IF NOT EXISTS tablefunc;
```

This operation records catalog metadata; it does not load extension SQL, find an
installation, inspect user files, or install an extension in a database. Supply
extension definitions explicitly through the documented [catalog/configuration
workflow](../../installation.md) before relying on their functions, types or
operators.

## Dialect and engine-specific behavior

This is PostgreSQL extension metadata handling. Options retained by the parser,
including schema/version/cascade choices, do not establish extension
installation or complete option validation. SQLite runtime extensions and
function registration are different facilities and are not implemented by this
statement path.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  requested extension is already recorded and IF NOT EXISTS was not supplied.
