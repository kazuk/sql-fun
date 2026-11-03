# domain_base_type_not_found

## Code and severity

Code: `domain_base_type_not_found`. Severity: Error.

## When it is reported

CREATE DOMAIN names a base type that cannot be canonicalized in the selected
catalog.

## What analysis knows and does not know

The domain name, requested base type and available source span are retained. The
domain has no successfully resolved base type.

## How to address it

Correct or qualify the base type, or load its definition before creating the
domain.

## Example

PostgreSQL 18 builtin catalog; `missing_type` is absent. CREATE DOMAIN is a
PostgreSQL feature in this analysis path.

```sql
CREATE DOMAIN docs_bad_domain AS missing_type;
```

Use the intended existing type, for example `CREATE DOMAIN docs_bad_domain AS
integer`, only when that is the required domain base.

## Related diagnostics

[column_type_not_found](column_type_not_found.md) reports a column declaration
type is missing.
[ownership_argument_type_not_found](ownership_argument_type_not_found.md)
reports an ownership signature type is missing.
