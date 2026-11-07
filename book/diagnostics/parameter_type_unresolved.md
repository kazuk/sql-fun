# parameter_type_unresolved

## Code and severity

Code: `parameter_type_unresolved`. Severity: Error.

## When it is reported

The first occurrence cannot select one application input type and no explicit
input hint was supplied. Ambiguity reports the remaining local binding
candidates from sql-fun's seven standard input candidates. A later use does not
revise that choice.

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
SELECT ${src} IS NULL, ${src}::text AS value;
```

The first occurrence imposes no input-type requirement. The later cast does not
resolve its ambiguity. Explicitly choose a text input with:

```sql
SELECT ${src:text} IS NULL, ${src}::text AS value;
```

In contrast, `SELECT ${src}::text AS value` selects `text` at its first occurrence
without an extra hint. Named placeholders are sql-fun input syntax. Do not mix
them with native numbered placeholders in one input.

## Related diagnostics

[parameter_constraint](parameter_constraint.md) reports conflicting
requirements.
[inference_candidate_space_unsupported](inference_candidate_space_unsupported.md)
reports a limitation of the analysis required to infer a type. See also
[parameter bindings](../cli/analyze.md#parameter-bindings).
