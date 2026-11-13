# lint_configuration

## Code and severity

Code: `lint_configuration`. Severity: Warning.

## When it is reported

A project `[[lint.deprecated]]` entry cannot be applied because its kind is
unknown, its fields are inapplicable, or its object, column or enum label cannot
be resolved against the selected schema and search path. For example, `column`
is only applicable to `table` and `view` entries, and an `enum_label` entry must
supply an existing `label`.

This check occurs when project linting is explicitly applied with the
application schema and declarations loaded. CLI configuration transport is not
yet implemented ([#1088](https://github.com/kazuk/sql-fun/issues/1088)). Malformed
TOML and file read failures are configuration-loading errors, not this diagnostic.

## What analysis knows and does not know

The warning identifies the configured target and why the entry cannot be
applied. It disables that entry without broadening the target; other valid
entries still apply. A missing column never becomes a whole-table rule, and a
missing enum label never becomes a whole-type rule. This warning does not mean
the application SQL itself is invalid.

Configuration warnings do not satisfy `expect = true`, which asks for a
corresponding deprecated-reference warning. An unresolved entry retains that
expectation. End-of-period expectation reporting remains in
[#1087](https://github.com/kazuk/sql-fun/issues/1087).
A query source position is provided only when a reliable location is available.

## How to address it

Check the entry's kind, name components and optional fields against the schema
and search path selected for analysis. Correct the entry or remove it if the
target is no longer intended. Use an explicit schema component when necessary
to identify the intended object.

## Example

Suppose the selected PostgreSQL application schema contains:

```sql
CREATE TABLE public.items (id integer, legacy text);
```

Supply this declaration when applying project linting:

```toml
[[lint.deprecated]]
kind = "table"
name = ["public", "items"]
column = "old_legacy"
expect = true
```

Analyze this application query:

```sql
SELECT legacy FROM public.items;
```

The table exists, but its column `old_legacy` does not. Applying the declaration
produces `lint_configuration` with the reason that the configured table column
cannot be resolved. The query is valid and this entry produces no `deprecated`
warning, even though it references the table. The invalid column selector does
not turn into a rule for every reference to `public.items`.

Changing the declaration to `column = "legacy"` makes it applicable; the same
query then produces `deprecated` instead. See
[SQL Analysis Configuration](../analysis-configuration.md) for the full format.

## Related diagnostics

[deprecated](deprecated.md) reports a reference to an applicable configured
target. [duplicate_lint_target](duplicate_lint_target.md) reports repeated
entries that successfully resolve to the same target.
