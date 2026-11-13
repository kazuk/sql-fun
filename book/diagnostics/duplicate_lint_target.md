# duplicate_lint_target

## Code and severity

Code: `duplicate_lint_target`. Severity: Warning.

## When it is reported

Multiple project deprecated declarations resolve to the same object and
optional column or label. Their expectations are combined with logical OR.

## What analysis knows and does not know

The diagnostic identifies the configured target. A query source position is
provided only when a reliable location is available. Configuration warnings do
not satisfy an expected deprecated-reference warning.

## How to address it

Keep one declaration for the target. Qualified and unqualified names can
resolve to the same object.

## Example

See [SQL Analysis Configuration](../analysis-configuration.md) for declarations
and supported target kinds.

## Related diagnostics

See [deprecated](deprecated.md),
[duplicate_lint_target](duplicate_lint_target.md), and
[lint_configuration](lint_configuration.md) for the related project lint warnings.
