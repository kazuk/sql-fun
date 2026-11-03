# column_type_not_found

## Code and severity

Code: `column_type_not_found`. Severity: Error.

## When it is reported

A table or composite column declaration names a type that cannot be found in the
selected catalog.

## What analysis knows and does not know

The owning declaration, column name and requested type are retained. No declared
column type has been established.

## How to address it

Correct the type name or qualification, or load the definition of the intended
user type before the declaration.

## Example

PostgreSQL 18 builtin catalog; `missing_type` is absent. SQLite has different
declared-type rules.

```sql
CREATE TABLE docs_bad (id missing_type);
```

If an integer was intended, declare `id integer`; do not substitute a type
solely to silence analysis.

## Related diagnostics

[domain_base_type_not_found](domain_base_type_not_found.md) reports a domain's
base type is missing.
[table_function_type_not_found](table_function_type_not_found.md) reports a
table-function output declaration type is missing.
