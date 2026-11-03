# collation_name_ambiguous

## Code and severity

Code: `collation_name_ambiguous`. Severity: Error.

## When it is reported

Explicit collation lookup returns multiple matching definitions after the
applicable namespace visibility rules.

## What analysis knows and does not know

The requested collation name is retained. No unique collation binding is
established.

## How to address it

Qualify the intended collation name and check the selected catalog for competing
visible definitions.

## Example

Illustrative PostgreSQL input: the catalog lookup for `chosen_collation` must be
ambiguous. A normal catalog with one visible matching collation will not
reproduce it. SQLite uses its own collation lookup and this example does not
establish identical behavior there.

```sql
SELECT 'text' COLLATE chosen_collation;
```

## Related diagnostics

[collation_not_found](collation_not_found.md) finds no definition;
[collate_operand_type](collate_operand_type.md) concerns applicability after
lookup.
