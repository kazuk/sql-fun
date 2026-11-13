# deprecated

## Code and severity

Code: `deprecated`. Severity: Warning.

## When it is reported

An application query references an object selected by the project deprecated
lint configuration, including a specifically selected column.

## What analysis knows and does not know

The diagnostic identifies the configured target. A query source position is
provided only when a reliable location is available. Configuration warnings do
not satisfy an expected deprecated-reference warning.

## How to address it

Replace the reference with the intended supported schema object, or review
the explicitly selected project lint configuration.

## Example

See [SQL Analysis Configuration](../analysis-configuration.md) for declarations
and supported target kinds.

## Related diagnostics

See [deprecated](deprecated.md),
[duplicate_lint_target](duplicate_lint_target.md), and
[lint_configuration](lint_configuration.md) for the related project lint warnings.
