# alter_column_virtual_generated

## Code and severity

Code: `alter_column_virtual_generated`. Severity: Error.

## When it is reported

DROP EXPRESSION or SET STATISTICS targets a column whose retained generation
mode is virtual. DROP EXPRESSION IF EXISTS does not remove this restriction.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved. Virtual-generation metadata must already be available; SQL syntax
alone does not guarantee that the selected parser can create it.

## How to address it

Check the generation mode and select an operation supported for that mode. Do
not treat this as evidence that all generated columns reject the operation.

## Example

Illustrative PostgreSQL 18 case: the selected semantic catalog must already
describe `public.t.g` as a virtual generated integer column. This is not a
standalone CLI schema-loading recipe.

```sql
ALTER TABLE public.t ALTER COLUMN g SET STATISTICS 100;
```

The analyzer checks retained virtual-generation metadata. Parser support for
creating that metadata is a separate prerequisite.

## Related diagnostics

[alter_column_not_stored_generated](alter_column_not_stored_generated.md)
concerns DROP EXPRESSION without stored generation;
[alter_column_version](alter_column_version.md) concerns a version-specific
spelling.
