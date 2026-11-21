# query_analysis

## Code and severity

Code: `query_analysis`. Severity: Error.

## When it is reported

Statement analysis or requested access analysis returns an unrecoverable
analysis-stage failure. PostgreSQL schema-file analysis also reports this
code when an extension membership change names a missing extension, attempts
to add a member that already belongs to an extension, or attempts to remove
an object from an extension it does not belong to. Adding the extension's own
schema also reports this code when that schema is known.

## What analysis knows and does not know

The analysis-stage explanation is retained. Partial statement facts can exist,
but this diagnostic does not establish complete semantics or a database
execution result.

## How to address it

Read the stage explanation and correct the identified input problem where
applicable. For unsupported or internal analysis failures, provide the selected
dialect/catalog and a minimal example when reporting the limitation.

## Example

PostgreSQL example requiring loaded project tables `items(id integer)` and
`incoming(id integer)`. The non-boolean MERGE condition exercises an
analysis-stage failure in the maintained schema-merge fixtures; it is not a
builtin-only example.

```sql
MERGE INTO items t USING incoming s ON 1
WHEN MATCHED THEN DELETE;
```

## Related diagnostics

[query_parse](query_parse.md) precedes semantic analysis;
[parameter_constraint](parameter_constraint.md) is a specific recoverable
type-constraint diagnostic.
