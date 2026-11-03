# conflicting_creation

## Code and severity

Code: `conflicting_creation`. Severity: Error.

## When it is reported

A schema creation conflicts with an existing identity in the applicable
namespace, including relations, named objects, routine signatures or
relation-scoped constraints.

## What analysis knows and does not know

The effective identity and namespace are retained, with requested/existing
kinds, signature, container or span when available. The diagnostic does not
authorize replacing the existing definition.

## How to address it

Choose a distinct intended name or signature, or explicitly use the supported
replacement/existence behavior for that object kind. Check the selected schema
for an earlier definition.

## Example

PostgreSQL schema-loading example: a table named `public.items` must already
exist in the selected schema. The statement attempts a second creation.
Namespace and duplicate rules vary by object kind and dialect.

```sql
CREATE TABLE items (id integer);
```

## Related diagnostics

[relation_already_exists](relation_already_exists.md) is a relation-name
diagnostic; [rule_invalid_definition](rule_invalid_definition.md) includes
rule-specific duplicates.
