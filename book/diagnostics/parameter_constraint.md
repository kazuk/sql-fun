# parameter_constraint

## Code and severity

Code: `parameter_constraint`. Severity: Error.

## When it is reported

SQL-derived parameter/type requirements cannot be reconciled, or a supported
explicit hint conflicts with those requirements. A later use can also reject
the binding selected at the first occurrence. In that case the diagnostic
identifies the selected type and the conflicting type requirements.

## What analysis knows and does not know

The constraint failure explanation is retained. An impossible domain is
different from a domain that still admits several possible input types.

## How to address it

Check every occurrence of the parameter and all supplied hints. Make the
intended operations compatible; adding a hint cannot make contradictory
requirements consistent.

## Example

PostgreSQL 18 builtin catalog; no project tables are needed. This example can
also report unreachable operands and an unresolved final input binding.

```sql
SELECT COALESCE(1, $1, true);
```

## Related diagnostics

[parameter_type_unresolved](parameter_type_unresolved.md) lacks one final input
type;
[inference_candidate_space_unsupported](inference_candidate_space_unsupported.md)
reports an unsupported search.
