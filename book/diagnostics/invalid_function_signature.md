# invalid_function_signature

## Code and severity

The error code is `invalid_function_signature`.

## When it is reported

PostgreSQL function analysis reports this error when a declaration has an
invalid signature. Examples include an unresolved parameter type in an
implicit shell declaration, a missing schema for the shell, a type modifier
on a shell return type, or a SQL-language function returning a shell.

## What analysis knows and does not know

Analysis records the function name and the reason it rejected the signature
using the selected catalog and schema. The rejection leaves the catalog
unchanged, including any shell the function would have introduced. Routine
body validation and native symbol loading are PostgreSQL runtime checks.

## How to address it

Check the parameter and return types against the selected schema. Declare each
parameter type before the function that uses it. For an implicit shell,
choose an existing creation schema and omit type modifiers from the shell
return. Complete the type before declaring a SQL-language function that
returns it.

## Example

Analyze the following statement with a PostgreSQL 17 or 18 builtin catalog
and a selected schema containing `public`. Neither `public.missing_arg` nor
`public.missing_result` should exist. The internal-language declaration could
introduce its return shell, but the missing parameter type prevents the
declaration from being applied.

```sql
CREATE FUNCTION public.bad_signature(public.missing_arg)
RETURNS public.missing_result
LANGUAGE internal AS 'textin';
```

## Related diagnostics

[conflicting_creation](conflicting_creation.md) reports a duplicate routine
signature or a type-name collision.
[base_type_invalid_definition](base_type_invalid_definition.md) reports a
rejected base-type definition when completing a shell.
