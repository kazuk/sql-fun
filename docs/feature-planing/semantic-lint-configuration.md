# Semantic Lint Configuration

*This is a planning document outlining a potential future feature of
`sql-fun`.
It has not been implemented, and no timeline or commitment has been
established.*

## Motivation

`sql-fun` already resolves SQL syntax into semantic entities such as schema
objects, functions, operators, and casts. That semantic resolution can also be
used to attach project or engine-specific guidance to the resolved entities.

The goal is a lint system that operates on resolved SQL semantics rather than
text patterns. Legal SQL remains legal: lint rules report guidance and project
policy; they do not change SQL parsing or semantic validity.

Examples include:

- Recommend an indexable operator when a function form cannot use the
  corresponding index strategy, such as recommending a `pg_trgm` word
  similarity operator for suitable uses of `word_similarity`.
- Mark a project table, column, function, or operator as deprecated.
- Require an otherwise legal implicit cast to be written explicitly.
- Preserve project-specific SQL guidelines as configuration.

## Configuration Sources

Lint knowledge could come from both:

- `sql-fun-home`, for engine/version/extension-specific knowledge.
- The user project, for schema-specific deprecations and persistent project
  guidelines.

Both sources should use the same semantic identity used by normal schema,
function, operator, and cast lookup. A trie/map keyed in the same namespace as
semantic lookup can make lint lookup a companion operation to resolution.

Engine-provided rules and project-local rules can then be overlaid without
embedding individual lint cases in the resolver.

## Semantic Matches

Resolving an entity can return the lint configurations that match it together
with per-match context. Sem nodes should expose these matches through a common
API conceptually similar to:

```rust
trait SemNode {
    fn diagnostics(&self) -> &[LintMatch];
}

struct LintMatch {
    configuration: LintConfigurationId,
    context: LintMatchContext,
}
```

The configuration is immutable rule knowledge. The match context contains the
semantic information produced by this particular resolution and any source
location required to report or rewrite it.

This also covers semantic nodes that do not exist explicitly in source SQL.
For example, if semantic analysis creates an implicit-cast expression, a rule
can match that resolved cast without attempting to infer it from source text.

## Match Counts and Negative Conditions

Loaded lint configuration may keep runtime-only state such as a match count.
The count does not need to exist in the configuration file.

A rule may optionally define behavior for the case where it receives no
matches, conceptually:

```toml
condition_not_met = "warn"
```

This supports transitional rules. For example, a project can mark a column as
deprecated and warn on every remaining reference. Once the project-wide query
inventory contains zero references, the same rule can report that its
deprecation condition is no longer matched and the migration can proceed to
cleanup.

This is deliberately different from claiming that a database column is always
safe to drop. The observation is limited to the query/schema ownership visible
to the configured `sql-fun` project.

Persistent project guidelines do not need `condition_not_met`; they can remain
in configuration indefinitely.

Match counts also make aggregate reporting possible, for example showing how
many times each configured rule matched during a verification run.

The current semantic analysis implementation is single-threaded, so an ordinary
numeric counter is sufficient. Any future parallel verification design should
reconsider runtime aggregation together with the broader SemAST/schema
concurrency model rather than introducing synchronization only for lint counts.

## Rewrite Metadata

A lint rule may optionally carry rewrite metadata. A matched rule with enough
source and semantic context could then support both diagnostics and explicit
fixing:

```text
sql-fun lint
sql-fun lint --fix
```

Rewrite applicability must distinguish mechanically safe transformations from
conditional or semantics-changing suggestions.

For example, replacing:

```sql
word_similarity($1, title) > 0.6
```

with a `pg_trgm` word-similarity operator is not unconditionally
semantics-preserving because the operator threshold is controlled separately.
Such a rule can still recommend the operator without claiming that an automatic
rewrite is safe.

The same rewrite metadata could later be consumed by LSP quick fixes.

## Project Deprecation Lifecycle

A project-local deprecation can use the same generic mechanism:

```text
add deprecation rule
        |
        v
references produce lint matches
        |
        v
match count decreases as callers migrate
        |
        v
match count == 0
        |
        v
condition_not_met diagnostic
        |
        v
remove the obsolete schema element and lint rule
```

No dedicated `deprecated` primitive is required by the semantic analyzer if
this behavior can be expressed by normal lint configuration.

## Scope

This proposal is about attaching lint knowledge to semantic resolution and
consuming the resulting matches. It does not propose:

- Rejecting SQL that is legal according to the selected SQL engine/specification.
- Inferring application deployment or migration ordering.
- Treating a zero project reference count as proof that arbitrary external
  database clients cannot reference an object.
- Making SemAST or schema analysis concurrent as part of the lint feature.

Queries continue to be verified against their selected datasource. If a query
does not specify a datasource, the default datasource and its schema dump are
used. Applications that need to verify queries against distinct schema states
can model those states as distinct datasources and select them explicitly.
Correctly executing a query against the corresponding runtime database state
remains the application's responsibility.
