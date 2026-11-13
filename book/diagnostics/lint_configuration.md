# lint_configuration

## Code and severity

Code: `lint_configuration`. Severity: Warning.

## When it is reported

A project deprecated entry cannot be applied: its kind or fields are
inapplicable, or the selected object, column, or enum label cannot be resolved.

## What analysis knows and does not know

The diagnostic identifies the configured target. A query source position is
provided only when a reliable location is available. Configuration warnings do
not satisfy an expected deprecated-reference warning.

## How to address it

Correct the entry against the selected schema and search path. The entry
is disabled without broadening its target; other entries still apply.

## Example

See [SQL Analysis Configuration](../analysis-configuration.md) for declarations
and supported target kinds.

## Related diagnostics

See [deprecated](deprecated.md),
[duplicate_lint_target](duplicate_lint_target.md), and
[lint_configuration](lint_configuration.md) for the related project lint warnings.
