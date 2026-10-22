# Repository instructions

## Scripts

Various utility scripts are located under `.local/bin/`.

- `check-test-flow`: Runs the standard build and test flow.
- `big-source`: Lists large source files based on their size (line count).
- `old-source`: Lists the 30 tracked Rust source files with the oldest
  last-change dates.
- `update-stats`: Generates a local coverage summary in `stats/coverage.txt`.

## Repository skills

Repository-local skills are stored under `.codex/skills/`. Read the relevant
skill's `SKILL.md` for its requirements and detailed workflow before using it.

- [implement-unit-test](.codex/skills/implement-unit-test/SKILL.md): Implement
  Rust unit tests for a specified source file when working on coverage-driven
  test targets.
- [old-source-review](.codex/skills/old-source-review/SKILL.md): Review one old
  Rust source file and create a maintenance issue for a maintainer decision.

## Build and Test

You can build and run tests using the `check-test-flow` script.

- Before submitting a pull request, run `.local/bin/check-test-flow` without
  `--step`. Targeted `--step` runs are supplemental checks for iterative
  development and debugging; they do not replace the full pre-PR verification.
- Run `check-test-flow` and `update-stats` sequentially, never concurrently.
  Their tests share temporary build directories and can interfere with each
  other's cleanup.
- When adding or modifying a CI job, do not declare the task complete until the
  corresponding job has finished on the final PR head and its result has been
  verified.

## Developer Workflow Compatibility

- Treat the documented contributor workflow as a compatibility contract.
  Changing required commands, prerequisites, or procedures in `CONTRIBUTING.md`
  affects all contributors and is a breaking change to the development workflow,
  even when the underlying code change is small.
- Internal changes to `check-test-flow` that preserve its documented entry
  point and contributor-facing behavior are routine implementation changes.
  Prefer absorbing new build and preparation steps there rather than exposing
  additional commands or manual prerequisites to contributors.
- Do not add developer-facing commands, bypass paths, or contributor procedures
  merely to accommodate internal implementation details. Document internal
  execution mechanics in their owning scripts rather than turning them into
  contributor obligations.
- Changes to the contributor workflow that require developers or agents to
  learn or perform different steps require an explicit maintainer decision.
  A feature implementation task alone does not authorize such a change.
  Clarifications that do not change contributor obligations remain routine
  documentation changes.

## PostgreSQL Behavioral Oracles

The development builder provides PostgreSQL 15–18 at `postgres15`, `postgres16`,
`postgres17`, and `postgres18`, each on port 5432. Authentication is preconfigured
through `PGUSER`, `PGPASSWORD`, and `PGDATABASE`; for example, use
`psql -h postgres18` from the builder, explicitly selecting the version to query.

Consult real PostgreSQL behavior when implementing or testing SQL parsing, type
resolution, implicit casts, function/operator overload resolution, NULL semantics,
and other PostgreSQL semantics. Compare versions where behavior may differ.
These servers are development oracles, not dependencies of ordinary or offline
tests. Use only small datasets needed to verify behavior. Bulk data loading,
large datasets, and load or stress testing are prohibited: database storage uses
tmpfs and consumes memory shared with the development environment.
See [the builder README](ci/github-actions/README.md#postgresql-development-oracles)
for operational details.

## Planning

- Before finalizing requirements or an implementation plan, trace each proposed
  behavior to the user's stated goal, its intended consumer, and an observable
  benefit. Check the actual end-to-end contracts, including the downstream API,
  protocol, and runtime, to establish which data they carry, preserve, and use.
  Verify relevant contracts against authoritative documentation or existing
  implementations; do not infer usefulness from SQL syntax or representability
  alone.
- For data that a downstream boundary cannot carry or use, identify a concrete
  purpose for accepting, interpreting, retaining, or validating it in this
  feature. Record that purpose and its consumer in the plan. Without such a
  purpose, flag the proposed behavior as unnecessary scope before implementation;
  syntax completeness or a hypothetical future consumer alone is insufficient.
- Identify the purpose of user-supplied detail before choosing its granularity.
  Hints compensate for a concrete analysis limitation: prefer making them
  unnecessary through supported analysis, or require only the shortest, simplest
  sufficient input. Excess specification increases user burden; richer hint
  syntax and finer control are not benefits by themselves. Audit and validation
  inputs serve a different purpose: detailed, explicit criteria can be valuable
  when users need to specify what must be checked. Evaluate detail against the
  actual task rather than treating either minimal or exhaustive specification
  as universally preferable, and record the intended use in the plan.
- When translating or expanding a maintainer's outline into an issue or plan,
  distinguish the maintainer's requirements from agent-proposed additions and
  assumptions. Translation does not authorize additional requirements. Surface
  requirements that lack a consumer benefit or conflict with downstream
  contracts, even if they already appear in the issue, and obtain a maintainer
  scope decision before implementing them. Do not silently expand or discard
  explicit requirements.
- During planning, check available disk space on the filesystem containing
  the worktree and build directories. Use 20 GB as the minimum baseline,
  and allow more if the planned builds or tests require it.
- Record the available capacity and estimated requirement in the plan.
  If capacity is insufficient, state that the user must resolve the
  shortage before implementation begins.
- Before finalizing an implementation plan, identify the technologies and
  patterns already used by the relevant modules and crates.
  Evaluate whether they can satisfy the new requirement, including the
  relevant dependency or framework contracts, and record the decision in the
  plan. Prefer extending an applicable existing approach over introducing a
  second strategy; propose a new strategy only when its technical advantage
  clearly outweighs its additional maintenance and conceptual cost.

## Unplanned Problems and Change Authorization

- Approval of an implementation plan authorizes only the work within that
  plan. Discovering a pre-existing defect, missing shared capability, or
  unexpected constraint does not authorize choosing or implementing a workaround
  or fix outside that scope.
- Prioritize completing the original task. When another problem is found,
  record concrete evidence, reproduction conditions, affected existing
  consumers, and its possible impact on the approved plan. Surface
  repository-wide correctness risks promptly; reporting a problem does not
  make its repair a prerequisite for the current task.
- Before proposing a prerequisite fix or pausing implementation, investigate
  the problem's actual effect on the original task's acceptance criteria and
  whether the task can be completed within its approved scope without fixing
  that problem. Judge blocking status by this dependency, not by the problem's
  severity, shared nature, or importance to other consumers.
- If the original task can be completed correctly within its approved scope,
  leave the separate problem untouched and continue through completion and PR
  submission. Then consult the maintainer about the separate problem's need
  for repair and priority. Do not require a separate fix PR to precede work
  that can be completed independently.
- Do not independently select, implement, or conceal an unplanned workaround,
  fallback, shared-service modification, or unrelated bug fix, regardless of its
  size or whether tests pass. Obtain an explicit maintainer decision on its
  scope and approach before proceeding with that work.
- Pause affected implementation and request a scope decision only when the
  original task cannot be completed correctly without resolving the separate
  problem. Identify the specific acceptance criterion that cannot be met,
  provide evidence of the dependency, explain why alternatives within the
  approved scope are insufficient, and request the narrowest necessary
  decision. Unaffected authorized work may continue. Do not silently weaken
  acceptance criteria or use an unapproved workaround to bypass the decision.
- This authorization boundary also applies during refactoring: the guidance
  below to avoid preserving known bugs does not grant permission to fix
  independently discovered, out-of-scope problems without approval.

## Crate Boundaries and Public APIs

- Define processing-specific data structures in the crate responsible for that
  processing. Do not move them into another crate solely to access private data.
- An API may be private simply because it has not had an external consumer yet.
  It is normal for a new implementation to become its first external consumer;
  privacy alone is not a reason to relocate the implementation.
- When an implementation needs an API from another crate that is currently
  private, prefer making the existing API public. If that is unsuitable, add
  the smallest public API needed.
- A consumer implementation MUST NOT modify the source crate's existing data
  model to accommodate consumer-specific processing needs or bypass private
  visibility. Keep the source model intact and derive processing-specific
  representations in the consuming crate. A genuine redesign of the source
  data model requires an explicitly agreed design scope, not an incidental
  change made to complete the consumer implementation.
- Continue the implementation without requiring prior approval for public API
  changes. Explicitly identify each newly exposed API and why it is needed in
  the pull request body; reviewers assess its signature and visibility through
  normal review.
- Preserve existing shared data representations and their serialization
  contracts when introducing processing-specific structures. Define new types
  for new purposes rather than repurposing existing shared types.

## External Tool Prerequisites

- When planning validation work, inspect the repository's standard checks with
  `check-test-flow --list-steps` before selecting a standalone validator or
  tool. When an existing check step covers the changed artifact or workflow,
  prefer that repository-standard step.
- When planning work that invokes or depends on an external tool, verify its
  availability in the development/CI Dockerfile
  (`ci/github-actions/Dockerfile`), contributor setup instructions
  (`CONTRIBUTING.md`), user installation documentation, and the current
  environment.
- Include any missing tool installation or setup documentation in the
  implementation plan. Clearly document required runtime tools as prerequisites
  for the affected CLI feature.

## CI and Workflow Design

- Before planning, implementing, or reviewing CI workflows, release automation,
  or their supporting helpers, read
  [docs/internal-rules/workflow-design-guideline.md](docs/internal-rules/workflow-design-guideline.md).
- Apply its rules for job boundaries, expected-failure validation, prerequisite
  gates, producer/provenance contracts, and helper placement.

## CI Image Maintenance

- Before modifying `ci/github-actions/Dockerfile`, CI image dependencies, or
  workflows that depend on the CI image, read [ci/README.md](ci/README.md).
- Follow the CI image update and adoption workflow documented there.

## Language

- English is the official language of the `sql-fun` repository. Use English when
  creating or updating issues, pull requests, discussions, review comments, and
  other repository communications, including their titles and bodies.
- Use English for commit messages, documentation, and code comments you add or
  modify.
- Use the user's preferred language for conversation.

## Branches

- Run `git fetch origin` before branch operations to refresh remote-tracking
  references. If fetching fails, resolve the failure before proceeding.
- Create new branches from `origin/<base-branch>` (for example, `origin/main`)
  rather than a potentially stale local base branch. This also applies when
  creating branches with `git worktree add`.
- In a Codex agent environment, do not invoke `git worktree add` directly.
  Use the `agent-worktree` repository skill and its `agent-worktree-add`
  helper, which creates linked worktrees below `$GIT_ROOT/.worktrees/`.

## Worktree Cleanup

- After a pull request is merged or a task is explicitly abandoned,
  remove the task's linked worktree.
- Before removal, verify that no uncommitted changes, untracked files,
  or unpushed commits need to be preserved. Account for squash merges when
  checking commits; ancestry alone does not establish whether work was merged.
- Do not remove worktrees used by active tasks or running processes.
- Use `git worktree remove` without `--force`.
- Removing the worktree also removes its task-local build artifacts,
  including `target/`. Do not run `cargo clean` separately.
- Never clean shared build directories or other tasks' worktrees.
- The agent handling the task is responsible for cleanup when it observes
  that the pull request has merged or the task has been abandoned.
- If the agent finishes before merge, report the worktree path and pending
  cleanup in the handoff. The next session handling that task must check
  the pull request status and perform cleanup when eligible.
- If removal is unsafe or cannot be completed, report the reason
  and leave the worktree intact. Otherwise, report cleanup completion.

## Commits

- Commits must follow the Conventional Commits specification.

## Versioning and Compatibility

- This project does not yet guarantee SemVer compatibility. Packages currently
  published on crates.io serve primarily as name reservations.
- During this phase, breaking API changes do not require compatibility shims,
  deprecated aliases, or version bumps solely for SemVer compliance. Do not
  request those changes in reviews solely on SemVer grounds.
- Package version bumps are handled during release operations; do not include
  them in feature or refactoring pull requests unless explicitly requested.
- Preserve explicit compatibility checks required by a feature, such as exact
  codegen version matching.

## Generated Code

- Before planning, implementing, or reviewing changes to code generators or
  templates, read and apply
  [docs/internal-rules/code-generator-guideline.md](docs/internal-rules/code-generator-guideline.md).
  Keep the detailed rules and rationale there rather than duplicating them here.

The following source file is generated automatically and must not be edited
directly:

- `sql-fun-sqlast/src/syn/generated.rs`
  - Generated by the `sql-fun-sqlast/generate` script.
  - The generation logic and templates live in the `xbuild` project.

## Issues

- Before creating or updating issues or pull requests, read
  [docs/internal-rules/issue-pull-request-labels.md](docs/internal-rules/issue-pull-request-labels.md)
  and apply its label semantics.
- Before creating an issue, read the templates in `.github/ISSUE_TEMPLATE/` and
  use the one that matches the issue: `bug_report.md` for bugs,
  `feature_request.md` for feature requests, or `maintenance.md` for refactoring
  and maintenance. If none applies, use a structure appropriate to the task
  rather than forcing it into an unrelated category.
- Include the selected template's sections and follow its title and label
  conventions. State when information is unknown or not applicable rather than
  inventing details.
- When updating an issue, preserve the applicable template structure and
  existing relevant information.

## Human-only work

- If progress stops because a required operation cannot be performed with the
  available agent, connector, API, permissions, or automation, follow the
  `require-human-work` rule in
  [docs/internal-rules/issue-pull-request-labels.md](docs/internal-rules/issue-pull-request-labels.md).
- Record the exact human action required and the automation limitation in a
  comment on the affected issue or pull request.
- After the human action is completed, continue the work when possible and
  remove `require-human-work` once continued work verifies that the automation
  boundary no longer blocks progress. Do not wait for the human to remove the
  label.

## Pull Requests

- Before creating or updating a pull request, read
  `.github/PULL_REQUEST_TEMPLATE.md` and include all of its sections in the
  description.
- Check only items that have actually been completed. Explain items that were
  not performed or are not applicable, and disclose any remaining validation
  gaps.
- Before waiting for PR CI, fetch the target branch and check the current
  PR head for merge conflicts and any requirement to be up to date.
- If merge conflicts or an up-to-date branch requirement prevent merging,
  rebase onto the latest target branch, resolve conflicts, run the
  appropriate local checks, and push the updated head before waiting for CI.
- Do not rebase merely because the target branch has advanced.
- Distinguish pending checks and required reviews from merge conflicts
  and an unmet up-to-date branch requirement.
- CI workflows here commonly take more than 10 minutes. While waiting, check
  GitHub Actions status no more often than once every 5 minutes, unless an
  immediate result is expected or a specific event requires verification.
  Prefer waiting for workflow completion over repeatedly querying individual
  jobs.
- Before reporting a PR as ready to merge, verify mergeability and required
  CI results for the current PR head.

## Review Guidelines

- When reviewing scripts under this repository, assume the contributor
  environment described in `CONTRIBUTING.md#Getting-Started` is already active.
  Do not report missing setup for environment values or tools established by
  that prerequisite unless the script is intended to run outside the repository
  development environment.

## Product Design Philosophies

- Before planning, implementing, or reviewing user-facing behavior, read
  [book/design-philosophies.md](book/design-philosophies.md). In particular,
  apply the Explicit-choice Philosophy before introducing defaults, inference,
  automatic selection, fallback behavior, or other behavior that acts on an
  omitted choice.
- Omission is not consent. Do not infer or select a value merely because it is
  available, conventional, recommended, or the only valid option. A candidate
  may be suggested to the user, but it remains unselected until the user
  explicitly confirms or supplies it.
- Non-interactive or assume-yes modes do not authorize missing decisions. They
  may skip prompts only when all decisions required by that operation have been
  supplied explicitly through the operation's documented inputs.
- Inspecting user-owned assets is itself an action covered by explicit choice.
  Do not discover, open, parse, enumerate, or use project files, metadata,
  configuration, repositories, or other user-owned inputs merely because they
  are reachable from the current directory, environment, or another ambient
  source. The command's documented semantics or an explicit user choice must
  authorize that access.
- Treat ambient values as input only where the command contract explicitly says
  they are inputs. Their presence alone is not consent to inspect related user
  assets or to change the interaction mode. Likewise, an explicit asset or
  project selection authorizes the access needed for that selection; it does not
  implicitly authorize a broader scan of adjacent assets.
- When reviewing a convenience behavior, distinguish deriving a candidate from
  choosing it, and distinguish having access to information from having consent
  to inspect or use it. If either choice is missing, ask or require an explicit
  input rather than treating availability as agreement.

## User-Facing Documentation Planning and Review

- When planning user-facing documentation changes, identify the intended reader,
  the task or decision the information supports, and the appropriate document
  for it.
- Review documentation requirements from issues against that reader's needs
  during planning. If a requirement asks for implementation details without a
  user-facing purpose, flag the mismatch in the plan and propose keeping it in
  internal documentation or tests instead. Do not silently discard explicit
  requirements.
- Document observable behavior, usage, prerequisites, limitations, and naming
  rules that users must follow. Include implementation details only when they
  help users use the feature, make a decision, or troubleshoot a problem;
  internal correctness requirements do not automatically belong in the manual.
- During plan review, verify the proposed documentation scope and placement
  before implementation begins. For each planned addition, check whether it
  changes what the reader needs to know or do. Recheck this when reviewing the
  final documentation diff.

## SQL Analysis Context Architecture

- Before planning, implementing, or reviewing type resolution, type ownership,
  casts, overload resolution, or parameter inference in `sql-fun-sqlast`, read
  [docs/internal-rules/sqlast-type-system-guideline.md](docs/internal-rules/sqlast-type-system-guideline.md).
  Keep the representation, lifetime, constraint, and test-context guidance there.
- Before planning, implementing, or reviewing SQL analyzers, name resolution,
  semantic analysis, type/constraint inference, or their diagnostic and lint
  behavior, read and apply
  [docs/internal-rules/sql-analysis-context-guideline.md](docs/internal-rules/sql-analysis-context-guideline.md).
- `ParseContext` is the mandatory shared environment for every SQL analyzer;
  local scope or pass-state objects may supplement it but must not replace it.
  Keep the detailed contract and rationale in the linked guideline.

## Coding Guidelines and Reviews

- Before planning, implementing, or reviewing AST mutation APIs, read and apply
  [docs/internal-rules/ast-mutation-api-guideline.md](docs/internal-rules/ast-mutation-api-guideline.md).
  Keep the implementation and review guidance there rather than duplicating it
  in this file.
- Before implementing or modifying Rust code, including tests, read
  `docs/internal-rules/rust-coding-guideline.md` and identify the rules that
  apply to the change.
- Keep syntax-specific interpretation thin by composing shared semantic
  operations. Extend the responsible shared types when an operation is missing;
  do not duplicate shared decisions or collection logic in individual syntax
  handlers to avoid introducing syntax-specific dependencies into shared code.
  Apply the responsibility and review rules in the "Grow shared operations"
  section of
  [the Rust coding guideline](docs/internal-rules/rust-coding-guideline.md#grow-shared-operations).
- In particular, test placement follows a repository-specific convention. Before
  adding or moving tests, read and apply the "Test placement" section of that
  guideline.
- Before committing or requesting user review, review the diff against those
  rules and fix any violations. Include test code, constructors, and callers in
  this review. A successful build or test run does not establish guideline
  compliance.
- Existing code is not a justification for repeating a pattern that conflicts
  with the guidelines. Keep corrections within the scope of the task rather than
  expanding into unrelated cleanup.
- Refactoring is not required to preserve known-bad behavior. Do not
  intentionally reintroduce a bug solely to match pre-refactoring behavior. If
  the corrected behavior can be verified against the relevant specification or
  another authoritative source, prefer the fix, add regression coverage, and
  report the behavioral change. When the bug fix can be separated cheaply,
  prefer fixing it first in a focused branch or pull request and continue the
  refactoring from that corrected baseline. If correctness cannot be established
  within the task scope, surface the behavior change for review instead of
  silently choosing either behavior.
- Regression tests for bugs discovered during refactoring should target an
  enduring public or semantic behavior boundary. Do not add characterization
  tests that unnecessarily freeze parser-specific traversal, temporary migration
  scaffolding, or other implementation boundaries the refactoring is intended
  to remove.
- Before adding a permanent automated test, identify its maintained execution
  path, such as the standard local flow, CI, or another regularly invoked test
  suite. A test without a maintained execution path is dead code, not coverage.
  Temporary test harnesses, fixtures, and comparison scripts are allowed while
  developing or validating a change, but remove them before marking the pull
  request ready for review unless they have a long-term execution owner.
- Explicit user instructions take precedence over the guidelines. Explain any
  necessary departure from the guidelines and its reason; do not silently treat
  a task-specific decision as a repository-wide rule.
- Keep Rust coding rules and their rationale in the coding guideline rather than
  duplicating them here. Prefer automated lint enforcement for mechanically
  checkable rules; manually review the design requirements that linting cannot
  verify.

## Lookup dependency documentation

- When consulting Rust dependency documentation, first determine the resolved
  crate version from Cargo.lock. If Cargo.lock is unavailable or the dependency
  is not locked, inspect the relevant Cargo.toml version requirement before
  using docs.rs.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community
structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or
instructions before doing anything else.

Rules:

- For codebase questions, first run `graphify query "<question>"` when
  graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for
  relationships and `graphify explain "<concept>"` for focused concepts. These
  return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw
  grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates;
  dirty graph files are not a reason to skip graphify. Only skip graphify if the
  task is about stale or incorrect graph output, or the user explicitly says not
  to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of
  raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when
  query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current
  (AST-only, no API cost).
