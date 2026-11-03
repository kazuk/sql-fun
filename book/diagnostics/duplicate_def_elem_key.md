# duplicate_def_elem_key

## Code and severity

Code: `duplicate_def_elem_key`. Severity: Error.

## When it is reported

A parsed definition-option list repeats the same option key. The later entry is
encountered after the key has already been collected.

## What analysis knows and does not know

The repeated key is retained. This diagnostic does not decide whether either
supplied option value is itself valid.

## How to address it

Specify each option once in the relevant list and keep the intended value. Check
statement-specific diagnostics for unsupported names or values.

## Example

Illustrative PostgreSQL option-list shape. This code is emitted by the public
option-list analysis API when supplied a list containing repeated keys. Current
CLI handlers do not route this SQL through that helper; this is not a CLI
reproducer.

```sql
CREATE FUNCTION f() RETURNS integer
LANGUAGE sql LANGUAGE sql AS 'SELECT 1';
```

## Related diagnostics

[alter_column_duplicate_option](alter_column_duplicate_option.md) is specific to
ALTER COLUMN options; [conflicting_creation](conflicting_creation.md) concerns
existing schema identities.
