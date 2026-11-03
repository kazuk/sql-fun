# alter_column_identity

## Code and severity

Code: `alter_column_identity`. Severity: Error.

## When it is reported

DROP NOT NULL targets a column declared with an identity constraint.

## What analysis knows and does not know

The table and column names and the reason are retained. A rejected alteration
does not establish a new column definition; the existing catalog state is
preserved.

## How to address it

Keep the identity column non-null, or deliberately revise the identity design
before changing nullability.

## Example

PostgreSQL; the schema contains `public.identity_table(id integer GENERATED
ALWAYS AS IDENTITY)`.

```sql
ALTER TABLE public.identity_table ALTER COLUMN id DROP NOT NULL;
```

Identity status, rather than a guessed data value, establishes the restriction.

## Related diagnostics

[alter_column_primary_key](alter_column_primary_key.md) concerns primary-key
nullability; both reasons can apply to one column.
