# CREATE EXTENSION

## Overview

The PostgreSQL schema analyzer records the requested extension name, the
explicit SCHEMA option when present, and supported IF NOT EXISTS behavior.
A duplicate without that explicit flag reports
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

## Datasource extension loading

When [analyzing configured extensions](../../cli/analyze.md#schema-loading),
sql-fun can load an installation script and its update scripts in sequence
to reach the selected or declared default version. When there is no declared
default, specify a version from the usable versions listed in the diagnostic.
sql-fun also loads dependencies declared in the selected extensions' control
files. Each script can use the installation schema and definitions of its
prerequisites, including when an update introduces a new prerequisite.

See [Bundled extensions](../../postgres-extensions/bundled.md) for verified
versions and extension-specific limits. For example,
[earthdistance](../../postgres-extensions/bundled/earthdistance.md) loads
its `cube` prerequisite, but the `earth_distance(earth, earth)` call can still
fail overload resolution ([#1304](https://github.com/kazuk/sql-fun/issues/1304)).
[PL/pgSQL](../../postgres-extensions/bundled/plpgsql.md) requires its assumed
installation owner to appear in the datasource role inventory.

## Diagnostics

- [conflicting_creation](../../diagnostics/conflicting_creation.md): the
  requested extension is already recorded and IF NOT EXISTS was not supplied.
