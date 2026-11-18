# unmet_lint_expectation

## Code and severity

Code: `unmet_lint_expectation`. Severity: Warning.

## When it is reported

The Server API `report_lint_expectations` operation reports this warning when a
configured deprecated target has effective `expect = true` and no corresponding
`deprecated` diagnostic has occurred for the supplied schema context handle.
It can be reported before any query has been analyzed.

## What analysis knows and does not know

The warning identifies the configured target and lint configuration file without
a SQL reference span. Unresolved or uninterpretable entries remain eligible;
a `lint_configuration` warning does not satisfy the expectation. Resolved
duplicates have one effective target and logical-OR `expect`.

This is the current observation state, not a claim that the application never
references the target. The caller determines SQL inventory completeness and how
to handle failed analyses. Reporting does not freeze, reset, or invalidate the
context. Additional query analysis can satisfy the expectation before a later
report. Earlier diagnostic collections remain unchanged.

## How to address it

Check that the intended queries have been analyzed. If the deprecated target no
longer needs tracking, remove or update the obsolete declaration. If a reference
is optional, remove `expect = true` or explicitly set it to false; references
still produce `deprecated` warnings. Initialize a new context to pick up edits
to the lint configuration.

## Example

Suppose the selected schema contains `public.items`, and the lint file contains:

```toml
[[lint.deprecated]]
kind = "table"
name = ["public", "items"]
expect = true
```

Reporting before analyzing any query returns `unmet_lint_expectation`. Analyze
`SELECT * FROM public.items` using the same context handle to produce a
`deprecated` warning. A subsequent report no longer includes that unmet
expectation. Repeating either report does not reset observations.

## Related diagnostics

[deprecated](deprecated.md) satisfies a matching expectation.
[lint_configuration](lint_configuration.md) and
[duplicate_lint_target](duplicate_lint_target.md) explain configuration warnings.
See [SQL Analysis Configuration](../analysis-configuration.md) and the
[Server API workflow](../api/check-analysis-design.md#report-current-lint-expectations).
See [lint expectations in `sql-fun check`](../cli/check.md#lint-expectations)
for exit status and the override option.
