# SQL Analysis Context Architecture

This document defines the environment boundary for SQL analyzers in
`sql-fun-sqlast`. It applies when planning, implementing, and reviewing
SQL parsing-to-semantics, name resolution, type/constraint inference,
semantic validation, and diagnostic/lint processing.

## Shared analysis environment

**Every SQL analyzer MUST receive and use `ParseContext` as its common
analysis environment.** Passing it through the analysis call chain is an
intentional architectural requirement, even if a particular analyzer does
not currently consume all of its capabilities.

`ParseContext` is the distribution route for analysis-wide environment
and configuration constructed from core `Args` and `Metadata`. This
includes database catalog and search-path access, the selected engine and
SQL dialect, diagnostic policy, future lint configuration, and other
settings that can affect analyzer behavior. Future settings must become
available to all analyzers through this same route without changing the
signatures of each intermediate resolver or visitor.

- Do not replace or bypass `ParseContext` with a second shared environment
  interface or object exposing a selected subset of its services.
- Do not create parallel catalog or diagnostic contexts for analyzers as
  substitutes for `ParseContext`.
- Do not propagate shared configuration (such as `Dialect`) as independent
  arguments throughout the analyzer call graph. Obtain it from the common
  environment instead. Verify the contract between configured metadata and
  selected dialect rather than assuming they always coincide.
- The fact that an analyzer currently needs only a few catalog operations,
  has no need for a particular setting, or would be easier to implement with
  a smaller interface does not justify omitting `ParseContext`.

## Local scope and transient analysis state

Additional context objects are permitted **alongside** `ParseContext`
when they represent narrower, explicitly scoped responsibilities.

`ScalarAnalysisContext` is an intentional example: it carries
expression-local FROM/CTE bindings and enclosing query scopes. This is
distinct from database-level configuration and does not replace
`ParseContext`.

Temporary state belonging to a resolution/inference pass may likewise be
represented separately, provided it does not become another provider of
the shared environment. In particular, splitting name resolution from
parameter-constraint collection/solving/finalization must not split their
access to `ParseContext`.

## Semantic AST and name-resolution completeness

**Retaining syntax is not the same as completing semantic analysis or name
resolution.** A semantic AST must preserve the structured SQL constructs and
references needed by subsequent passes. Merely copying an identifier or making
an expression traversable does not constitute name resolution.

For every name reference retained in the semantic AST:

- Apply the applicable SQL dialect's visibility, scoping, precedence, and
  ambiguity rules. **Every reference MUST be subjected to name resolution and
  its outcome recorded**, including references in query clauses, CTEs,
  windows, grouping, ordering, and function modifiers.
- Represent successful resolution with a semantic type identifying the kind
  of definition referenced, rather than an unclassified name. For example,
  `OVER w` must resolve `w` against the applicable `WINDOW` definitions
  and retain a typed window-alias binding.
- Retain unresolved and ambiguous references where appropriate, but encode
  each as an explicit, distinguishable completed lookup outcome (such as
  `NotFound` or `Ambiguous`). A failed lookup is not an excuse to skip
  resolution or discard the reference.
- Subsequent semantic visitors and type-constraint passes MUST consume the
  recorded outcomes without repeating name lookup. Type inference and
  additional SQL legality validation are distinct phases; neither should
  masquerade as completed name resolution.

**A name-resolution implementation is complete only when every applicable
reference has been visited and its lookup outcome represented**, regardless
of whether resolution succeeded. A semantic representation or resolver that
silently substitutes empty nodes, placeholder success, or unclassified names
for unsupported behavior MUST NOT be presented as a completed implementation.
Track independently actionable missing behavior in issues instead of hiding
it; uncommon but valid SQL syntax is not grounds for excluding support.

These requirements apply to each incremental SQL semantic-analysis issue,
including its acceptance criteria and regression tests. Tests must inspect
the resulting semantic bindings and explicit failure/ambiguity outcomes,
not merely successful traversal or retained syntax. See the
[name-resolution completion clarification in #463](https://github.com/kazuk/sql-fun/issues/463#issuecomment-6226367685).

## Diagnostics and future lint configuration

Diagnostic policy is analysis-wide configuration, not an implementation
detail of a particular resolver. All analyzers must use the shared
environment so future lint policies can be applied consistently at the
point where each semantic problem is reported.

For example, an `expect`-style lint policy may need to suppress a known
finding, record that the expected finding occurred, and report an
unfulfilled expectation when the relevant analysis scope completes.
A resolver-local substitute for `ParseContext` must not force that policy
or its state to be copied across analysis paths or supplied as extra
parameters.

Diagnostic problems should retain structured semantic identities; do not
encode error kinds solely as free-form strings to make them pass through a
simplified reporting interface.

## Design and review checks

Before introducing or changing an analyzer context or an environment
parameter, confirm:

- Does every analyzer path still receive `ParseContext`?
- Is this additional state genuinely query-local or pass-local, rather
  than a replacement for shared analysis environment services?
- Can a future configuration value originating in core `Args` /
  `Metadata` reach this analyzer without adding parameters to every
  intermediate call?
- Are diagnostic reporting and lint-policy decisions available through
  the same shared environment?
- Does a refactor preserve established semantic phases and their outputs?

An existing implementation that bypasses this contract is technical debt,
not a precedent for introducing another bypass. See issue
[#470](https://github.com/kazuk/sql-fun/issues/470) for the planned
restoration of this boundary.
