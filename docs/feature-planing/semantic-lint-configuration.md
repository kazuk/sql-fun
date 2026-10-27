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

### Lint Groups

Lint configuration should not be constrained to a single configuration file.
It should be possible to organize rules into multiple lint groups and select
which groups are enabled for a project.

For example, PostgreSQL-specific knowledge could be grouped by extension so
that lint rules for an extension such as `pg_trgm` can be enabled or disabled
as a group. A lint group should also be able to have its own configuration
file, allowing built-in knowledge and project-specific rule sets to remain
separately maintainable.

The exact file layout, loading rules, and enable/disable configuration are
implementation-planning details. The proposal only requires that the loading
model does not unnecessarily prevent this grouping and selection.

## Resolution-Time Attachment

Lint lookup should happen as part of semantic name resolution, not as a later
textual or SemAST re-matching pass. When the resolver looks up a schema object,
function, operator, or cast using its canonical semantic key, it can use the
same key to look up configured lint rules.

Conceptually:

```text
syntax node
    |
    v
semantic/name resolution
    |-- catalog lookup -----> resolved entity
    |-- lint trie lookup ---> matching configurations
    |
    v
Sem node {
    resolved entity,
    lint matches: [
        configuration + resolution/match context
    ]
}
```

The context captured at this point should contain the information that made the
match meaningful: the resolved identity, relevant argument/type bindings,
source span, and other semantic facts needed by diagnostics or rewrites. This
avoids reconstructing resolution decisions later and also allows generated
semantic nodes, such as implicit casts, to carry lint matches when they are
created.

The resolver does not need to understand the policy expressed by each lint. Its
responsibility is to resolve the semantic entity, retrieve rules keyed to that
entity, capture the resolution-time context, and attach the resulting matches
to the Sem node. Diagnostic interpretation remains a downstream concern.

## Expected Implementation Shape

This proposal does not fix the implementation plan in advance; the actual work
should begin with a fresh plan against the codebase at that time. Based on the
current architecture, however, the expected implementation shape has two
parts.

First, `ParseContext` is currently also the schema context and is expected to
be the primary observation point. Semantic construction asks it to resolve
schema objects such as tables, columns, functions, operators, types, and casts.
The resolve request can perform the corresponding lint lookup and increment the
loaded lint node's match counter. This should cover implicit casts as well:
although the cast expression is created implicitly, semantic analysis still
asks the schema context to resolve whether the required cast exists.

Second, the change cannot be contained entirely inside `ParseContext`.
Individual SemAST nodes need storage for the resulting lint match/context, and
their construction paths need to carry that information from resolution into
the SemAST. Diagnostics, rewrites, and artifact generation can then consume the
stored semantic match without repeating name resolution.

Conceptually:

```text
                    ParseContext
               (current schema context)
                         |
              resolve schema object
                         |
              +----------+----------+
              |                     |
        catalog resolve         lint lookup
                                      |
                                match_count += 1
              |                     |
              +----------+----------+
                         |
                         v
                    SemAST node
                resolved semantics
                + lint match/context
```

Therefore the expected implementation is not a separate lint modification in
every name-resolution algorithm. Resolution-time matching is expected to be
concentrated mainly in `ParseContext`, while the structural change propagates
through SemAST types and construction paths because each relevant node must
retain the match information.

## Semantic Matches

A resolved Sem node can therefore retain the lint configurations that matched
during resolution together with their per-match context. Sem nodes should
expose these matches through a common API conceptually similar to:

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

Each loaded lint node can keep a runtime-only match counter that is not present
in the configuration file. Whenever semantic resolution attaches a match for
that lint node to a Sem node, the same operation increments the loaded lint
node's counter.

Conceptually:

```text
loaded lint node
    match_count = 0
          |
          | semantic resolution matches
          v
Sem node <- LintMatch(configuration + context)
          |
          +--> loaded lint node.match_count += 1
```

This makes the loaded lint configuration both the rule definition used during
resolution and the natural aggregation point for the current analysis run.
After all queries have been analyzed, lint reporting can enumerate the loaded
lint nodes directly and report the detection count for every rule without a
second traversal of the SemAST.

A count greater than zero gives the number of detected occurrences. A zero
count is equally useful: rules that define behavior for an unmet condition can
emit that diagnostic directly from the same aggregate state.

For example, a report could naturally contain information equivalent to:

```text
project.deprecated-users-old-name: 3 matches
project.require-explicit-cast-x: 0 matches
postgres.pg-trgm-word-similarity-index: 7 matches
```

A rule may optionally define behavior for the case where its final match count
is zero, conceptually:

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

## Lint Analysis Artifacts and Cross-Project Aggregation

The aggregate result of a lint analysis can also be emitted as an artifact.
This allows the same semantic observations to be combined outside a single
project or repository.

This is useful when multiple independently built applications share one
enterprise database:

```text
project A --lint--> artifact A --+
project B --lint--> artifact B --+--> aggregate --> shared schema policy
project C --lint--> artifact C --+
```

An artifact can identify the lint rule, datasource/schema identity, resolved
schema object identity, and observed match count. The exact artifact format is
left for future design; the important property is that it records semantic
observations rather than requiring the aggregator to re-analyze source SQL.

For example, deprecation of a shared column may produce:

```text
public.users.legacy_name

project-a: 0 matches
project-b: 0 matches
project-c: 4 matches
--------------------
aggregate: 4 matches
```

Once every artifact in an explicitly defined consumer set reports zero matches,
an enterprise-level policy can report that the known consumers no longer use
the deprecated schema object.

A zero count from one artifact is not proof of global non-use. Even an
aggregated zero is meaningful only for the set of consumers whose artifacts
were intentionally included. Unknown applications, ad-hoc SQL, database-side
objects, BI tools, or other clients remain outside that observation unless they
are represented in the declared ownership/consumer set.

This keeps the producer simple: each `sql-fun` project reports what its own
semantic analysis observed. An external consumer or aggregation step owns the
knowledge of which projects constitute the required coverage for a shared
database. `sql-fun` therefore does not need to infer repository topology,
deployment topology, or application ownership in order to support
cross-project schema-object deprecation.

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

- Rejecting SQL that is legal according to the selected SQL
  engine/specification.
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
