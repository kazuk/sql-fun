# base_type_invalid_definition

## Code and severity

Code: `base_type_invalid_definition`. Severity: Error.

## When it is reported

PostgreSQL analysis rejects a base type definition or an
`ALTER TYPE name SET (...)` statement. For example, the definition may
reference a missing `LIKE` source type or a support function with an invalid
signature. A settings change may request a storage strategy incompatible
with the type's layout.

## What analysis knows and does not know

The diagnostic names the type and explains the missing prerequisite, invalid
option, or invalid support function. Analysis checks declarations against the
available catalog. PostgreSQL validates the physical layout and enforces
permissions at execution, and analysis does not invoke the support functions.

A rejected statement leaves the analysis catalog unchanged. If a rejected
definition was completing an existing shell, the shell remains available to
function declarations and can be completed by a later valid definition.

## How to address it

Declare the shell and its support functions before the full definition. Match
each support function's required argument and return types. If the declaration
uses `LIKE`, make sure its source type is already defined and visible in the
search path, or qualify its name with the schema. For `ALTER TYPE` storage
changes, check whether the type's layout and current storage strategy permit
the requested change. See
[base type settings changes](../sql-analysis/statements/create-type.md#postgresql-base-type-settings-changes).

## Example

Analyze this statement with a PostgreSQL builtin catalog that includes base
type eligibility and storage metadata for `pg_catalog.int4`. The fixed-size
type requires `plain` storage, so the request for `extended` reports
`base_type_invalid_definition` and leaves the existing setting in place.

```sql
ALTER TYPE pg_catalog.int4 SET (STORAGE = extended);
```

## Related diagnostics

[invalid_function_signature](invalid_function_signature.md) reports a rejected
function declaration. [conflicting_creation](conflicting_creation.md) reports
a declaration that conflicts with an existing object identity.
