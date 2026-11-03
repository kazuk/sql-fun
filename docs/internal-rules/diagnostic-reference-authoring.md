# Diagnostic reference authoring

This internal guide owns the writing baseline for `book/diagnostics/` (#922).
Its reader is a page author or reviewer. User pages explain observable behavior,
not Rust implementation details. The inventory and producer evidence are in [the
reference inventory](diagnostic-reference-inventory.md).

## Template

Use an exact-code H1 followed by these H2 headings, in this order:

1. Code and severity
2. When it is reported
3. What analysis knows and does not know
4. How to address it
5. Example
6. Related diagnostics

The first section states the exact code and severity, for example
``Code: `array_empty`. Severity: Error.``
State conditional severities and their exact conditions if introduced. Currently
no emitted code has conditional severity: index and ALTER COLUMN outcomes use
distinct codes for errors and warnings. There is no emitted advice code.

Every page needs the trigger, retained facts and uncertainty, actionable
remedies, a minimal example with schema/catalog/dialect/version prerequisites,
and relevant cross-code distinctions. State `Not applicable` and a reason if a
field does not apply. State what is unavailable and why when evidence is absent.
Never invent a reproducer for a metadata-dependent or unavailable condition:
identify the needed catalog state and clearly label an illustrative SQL input as
such.

## Conventions

- Write English. Distinguish a diagnostic, its code, and its severity.
- An unresolved fact is not a proven incompatibility. Unsupported analysis does
  not prove invalid SQL. Database validity and sql-fun behavior need separate
  evidence. The selected schema may differ from a live database.
- Qualify PostgreSQL/SQLite and version applicability. Do not claim SQLite
  runtime validation through the PostgreSQL-only initialization API. Shared
  semantic paths do not establish identical dialect behavior.
- Prefer supported analysis and the simplest sufficient input. Suggest hints
  only where their syntax and effect are supported; hints currently accept an
  identifier, not arbitrary SQL type syntax such as `int4[]`.
- Use fenced `sql` blocks, with prerequisites immediately before them. Separate
  failing input and a correction. Mention accompanying diagnostics when
  relevant; a representative example need not emit only one code.
- Link related pages with relative `<code>.md` paths and code labels. Explain
  distinctions rather than linking unrelated codes to fill a heading.
- Keep pages ordinary self-contained Markdown, suitable for raw retrieval and a
  future explain reader. Do not add HTML copies or an unneeded metadata format.

## Reviewed baseline and delegation gate

The parent agent prepared and reviewed these pilot pages against current source
and the worktree-built CLI/server with the PostgreSQL 18 builtin catalog:

- `column_not_found_in_source`: `SELECT missing AS result` reports that error;
  column type is unresolved.
- `parameter_type_unresolved`: `SELECT $1::text AS value` reports that error;
  result type is text. `SELECT ${src:text}::text AS value` completes with a text
  input and no diagnostic.
- `inference_candidate_space_unsupported`: `SELECT ($1)[1] IS DISTINCT FROM
  ($2)[1] AS result` reports that error and two unresolved input bindings;
  result type is bool.
- `unreachable_coalesce_argument`: `SELECT COALESCE(1, true)` reports a warning
  plus `coalesce_expr_type_unknown` as an independent error.

These cover name resolution, final input inference, unsupported analysis, and a
warning. No same-code conditional-severity pilot is applicable today. Sources
include the classifier in core, query finalization in server, query-source
lookup, parameter-constraint reporting and COALESCE common-type analysis in
SQLAST. The pilot correction using an array-syntax input hint was rejected by
validation and removed before finalizing this baseline; it is not supported
input syntax.

The template, conventions and reviewed pilot pages are the baseline to provide
to each sub-agent before assigning disjoint code groups. Every handoff must
include source evidence and example-validation results, with static-only and
runtime evidence distinguished. The parent owns inventory, index/navigation,
consistency checking, cross-page review and integrated validation. Update the
baseline and recheck affected pages if later evidence changes it.
