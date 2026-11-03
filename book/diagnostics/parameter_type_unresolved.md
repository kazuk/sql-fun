# parameter_type_unresolved

## Code and severity

Code: `parameter_type_unresolved`. Severity: Error.

## When it is reported

Finalization cannot establish one application input type for a parameter. Its
admissible domain may still contain multiple types or remain unrestricted.

## What analysis knows and does not know

The parameter index is known. Result-column types may already be known, but do
not establish a complete application input binding. This diagnostic does not
mean PostgreSQL rejects the SQL or cannot infer its own prepared-statement
signature.

## How to address it

First use supported SQL constraints to express the intended operation. Where
those constraints do not select a single input type, supply the supported
named-placeholder input hint with the shortest sufficient type. All uses of that
parameter must be compatible.

## Example

PostgreSQL 17–18 builtin schema.

```sql
SELECT $1::text AS value;
```

The result cast establishes a text result but can leave several input types
admissible here. To explicitly choose a text input, use:

```sql
SELECT ${src:text}::text AS value;
```

Named placeholders are sql-fun input syntax. Do not mix them with native
numbered placeholders in one input.

## Related diagnostics

[parameter_constraint](parameter_constraint.md) reports conflicting
requirements.
[inference_candidate_space_unsupported](inference_candidate_space_unsupported.md)
reports a limitation of the analysis required to infer a type. See also
[parameter bindings](../cli/analyze.md#parameter-bindings).
