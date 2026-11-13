# duplicate_lint_target

## Code and severity

Code: `duplicate_lint_target`. Severity: Warning.

## When it is reported

Multiple project `[[lint.deprecated]]` declarations resolve to the same target:
the same kind, resolved object name, and optional column or enum label. Duplicate
detection happens after name resolution, so qualified and unqualified names can
identify one target. A whole-table entry and an entry for one of its columns are
different targets.

This check occurs when project linting is explicitly applied with the
application schema and declarations loaded. CLI configuration transport is not
yet implemented ([#1088](https://github.com/kazuk/sql-fun/issues/1088)).

## What analysis knows and does not know

The warning identifies the repeated declaration's target. The declarations form
one effective target; its `expect` value is true if any duplicate explicitly
sets `expect = true`. A false or omitted value in another duplicate does not
cancel that expectation. Duplicate detection does not disable the target or
make otherwise valid application SQL invalid.

This configuration warning does not itself satisfy a reference expectation.
Only a corresponding `deprecated` warning does. End-of-period expectation
reporting remains in [#1087](https://github.com/kazuk/sql-fun/issues/1087).
A query source position is provided only when a reliable location is available.

## How to address it

Keep one declaration for the target, preserving `expect = true` if the
reference expectation is intended. Compare resolved identities, not just the
written names, when checking whether entries overlap.

## Example

Suppose the selected PostgreSQL application schema contains the following table,
and analysis uses the explicitly selected search path `public`:

```sql
CREATE TABLE public.items (id integer, legacy text);
```

Supply these declarations when applying project linting:

```toml
[[lint.deprecated]]
kind = "table"
name = ["items"]
column = "legacy"
expect = false

[[lint.deprecated]]
kind = "table"
name = ["public", "items"]
column = "legacy"
expect = true
```

Both entries resolve to `public.items.legacy`, so the second produces
`duplicate_lint_target`. The effective target retains `expect = true`.

```sql
SELECT legacy FROM public.items;
```

This query also produces `deprecated` for the column reference. That warning,
rather than the duplicate warning, satisfies the reference expectation for this
analysis. Remove the first entry to keep the same effective rule without the
duplicate warning. See [SQL Analysis Configuration](../analysis-configuration.md)
for the complete declaration format.

## Related diagnostics

[deprecated](deprecated.md) reports a reference to the effective target.
[lint_configuration](lint_configuration.md) reports an entry that cannot be
applied, rather than a successfully resolved duplicate.
