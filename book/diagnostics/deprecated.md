# deprecated

## Code and severity

Code: `deprecated`. Severity: Warning.

## When it is reported

An application query has a resolved reference to an object selected by the
project's `[[lint.deprecated]]` declarations. For a table or view, a declaration
with `column` selects only that column; omitting `column` selects the whole
relation. A function declaration selects all overloads of the resolved name.

Project linting must be explicitly applied with the declarations and application
schema loaded. CLI configuration transport is not yet implemented
([#1088](https://github.com/kazuk/sql-fun/issues/1088)).

## What analysis knows and does not know

The warning identifies the configured target, including its selected column
when applicable. References through aliases, wildcard expansion, CTEs and nested
queries can match the same target. The warning does not make otherwise valid SQL
invalid, and it does not identify a replacement object for the application.
A query source position is provided only when a reliable location is available.

`expect = true` records that a corresponding deprecated-reference warning is
required during the analysis period; it does not control whether this warning
is emitted. End-of-period expectation reporting remains in
[#1087](https://github.com/kazuk/sql-fun/issues/1087).

Valid `enum_label` declarations currently produce no deprecated-reference
warnings because enum-label references are not yet distinguished by analysis
([#1084](https://github.com/kazuk/sql-fun/issues/1084)).

## How to address it

Replace the reference with the intended supported schema object or column. If
the reference should remain allowed, review the explicitly selected project's
deprecation declaration. Changing `expect` alone does not disable the rule.

## Example

Suppose the selected PostgreSQL application schema contains:

```sql
CREATE TABLE public.items (id integer, legacy text);
```

Supply this lint declaration when applying project linting:

```toml
[[lint.deprecated]]
kind = "table"
name = ["public", "items"]
column = "legacy"
expect = true
```

Analyze this application query:

```sql
SELECT i.legacy FROM public.items AS i;
```

The resolved reference to `public.items.legacy` produces a `deprecated` warning
and satisfies this declaration's reference expectation for this analysis.
The alias `i` does not change the target. `SELECT id FROM public.items` does not
produce this warning: the declaration selects `legacy`, not the whole table.

See [SQL Analysis Configuration](../analysis-configuration.md) for the complete
declaration format and supported target kinds.

## Related diagnostics

[lint_configuration](lint_configuration.md) reports an entry that cannot be
applied. [duplicate_lint_target](duplicate_lint_target.md) reports multiple
entries resolving to one target. Neither configuration warning counts as a
deprecated-reference warning.
