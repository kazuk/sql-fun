# CI and Workflow Design Guideline

This guideline is for contributors planning, implementing, and reviewing CI
workflows, release automation, and their supporting helpers. It covers execution
boundaries, validation, and failure behavior. Feature-specific contracts still
define the inputs, provenance requirements, and operations for each workflow.

For CI image changes, follow the separate
[image maintenance workflow](../../ci/README.md).

## Validation and PR health

Expected-failure validation should not leave intentionally failing checks on
the PR head when an isolated validation ref can be used.

A correctly rejected input is useful evidence, but a failing Actions check
attached to the PR head makes the PR's health harder to interpret. Reviewers
should be able to distinguish implementation failures from successful
verification of rejection behavior without reconstructing the test history.

- Prefer offline contract tests that assert the expected command failure and
  pass when the rejection is correct.
- When actual workflow execution is necessary, use a disposable branch with a
  disposable SHA, or a dedicated manual-validation ref whose SHA differs from
  the PR head. A different branch name pointing to the same SHA does not
  isolate checks from the PR head.
- Preserve the relevant implementation when creating the isolated revision;
  for example, an empty commit can provide a distinct SHA with the same tree.
  If the workflow depends on revision-specific evidence, account for that
  dependency explicitly rather than borrowing evidence from another SHA.
- Confirm the validation ref's triggers, permissions, and possible side effects
  before running it. Isolating a ref does not itself prevent publication or
  other production operations.
- Record the tested revision, run link, expected failing step, and evidence that
  later steps did not run. Remove disposable refs after validation.
- If isolation cannot exercise the required behavior, explain that limitation
  and the expected failure in the PR. Keep genuine failures visible; do not
  weaken required checks or change production exit behavior to make the PR
  appear healthy.

Positive workflow validation must still cover the final PR head where required
by the repository's completion rules. An isolated negative run does not
replace final-head validation.

## Reject invalid transactions before proceeding

CI and release automation must stop transactions whose required choices,
validation evidence, or inputs do not satisfy their contract. Fail before
downloading, mutating, tagging, or publishing when a prerequisite for that
operation is unmet.

Use the operation's explicit inputs and documented selection rules. Missing
release identity, missing successful required CI, or missing required artifacts
must produce clear diagnostics. Report the selected repository, revision, run,
or input name as appropriate so the operator can correct the state.

Do not silently select another revision, fall back to older evidence, rebuild
a subset of outputs to bypass a validation gate, or invent missing release
choices. Any automatic recovery must be part of the operation's documented
contract and preserve its required inputs and guarantees. A retry must not
turn an ineligible transaction into an eligible one by relaxing the gate.

For example, when release preparation requires successful normal CI for the
exact target SHA, absence of that run is a release eligibility failure. The
operator can correct CI state and rerun for the same SHA, or explicitly push a
new target SHA that must satisfy the same requirement.

## Respect producer and provenance contracts

State which metadata and producer guarantees the consumer relies on. Reuse one
resolution path for inputs belonging to the same validated revision or CI run;
later inputs should extend that path rather than select their own provenance.

Adopt outputs according to their current producer contract. Require an input
only when its producer exists and the current transaction declares it required.
Check what is needed to locate and consume those outputs, and fail clearly when
required inputs are absent.

Do not repeat producer-side builds or validation, add duplicate provenance
manifests, or introduce another proof system merely because the consumer is a
release workflow. Additional validation must address a concrete failure mode
or an explicit contract requirement. Record repository-policy assumptions that
support the trust boundary and revisit them when those policies change.

### Repository-specific PR CI trust boundary

The active `main` ruleset requires branches to be up to date through
`strict_required_status_checks_policy: true`. A PR head behind the current
base must be updated or rebased before it can satisfy the required-check merge
gate. A successful PR CI run by itself does not establish merge eligibility.

GitHub's default checkout for a `pull_request` workflow uses the synthetic
`refs/pull/<n>/merge` commit. Its commit object can differ from the PR head.
For an up-to-date, mergeable PR head with successful required normal CI, this
repository's release contract relies on the synthetic merge commit having the
same source tree as that head. Commit-object identity and source-tree
equivalence are distinct; this is a repository-policy invariant, not a general
guarantee for arbitrary PR runs.

Release preparation deliberately relies on that invariant and GitHub Actions
metadata. It does not duplicate merge-policy validation inside the workflow
or add artifact-internal provenance proofs. The exact recorded target-SHA gate
still applies; tree equivalence does not authorize substituting a CI run
recorded for a different SHA.

When reviewing this assumption, inspect the current ruleset and checkout
semantics. Revisit the trust boundary if strict checks, merge eligibility,
CI events, or checkout behavior change so that the invariant no longer holds.
Do not assume every successful or historical PR run satisfies the
release-eligible state.

References:

- [Repository decision in PR #362](https://github.com/kazuk/sql-fun/pull/362#issuecomment-5990385136).
- [GitHub PR workflow semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).
- [GitHub strict status checks](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches#require-status-checks-before-merging).

## Choose job boundaries for execution needs

Use separate steps for sequential responsibilities that share release state,
permissions, runner requirements, and adopted files. A conceptual responsibility
split alone does not justify another job.

Introduce a job boundary when it provides a concrete execution benefit, such
as different permissions, a different runner, useful parallel execution, or
required isolation. Account for runner allocation, scheduling, repeated
checkout, and cross-job artifact transport when choosing that boundary.

Expose step outputs for subsequent work within a job and job outputs for
actual cross-job consumers. Keep shared inputs available in the same job when
later sequential steps can consume them directly. Do not introduce repeated
downloads or artifact handoffs just to preserve a conceptual decomposition.

## Place helpers by their supported invocation surface

The contributor environment adds `.local/bin/` to `PATH`. Place commands there
when direct contributor or maintainer invocation is an intended, supported
interface, including documented validation commands.

Place implementation helpers under `.local/sbin/`, or a suitable
automation-specific directory, when direct human invocation is not part of
their supported interface. Workflow use alone does not determine the location:
a supported maintainer command can also be called by Actions.

For example, workflow-only release identity, CI-run resolution, and input
adoption helpers belong under `.local/sbin/`. Invoke them by explicit path and
update workflow and test references when moving them. Do not expose internal
helpers on contributor `PATH` merely for convenience.

## Review and verification

Before requesting review, verify that:

- Job boundaries have an execution reason and preserve the intended ordering.
- Invalid choices or unmet prerequisites stop later dependent operations.
- Input selection, producer guarantees, and provenance assumptions are explicit.
- Internal helpers and supported human commands use the appropriate locations.
- Expected-failure runs are isolated from the PR head where possible, and their
  results remain available as review evidence.
- Relevant repository-standard checks pass, and changed jobs have completed on
  the final PR head as required by `AGENTS.md`.

Use the repository's existing validation and CI-image procedures rather than
introducing parallel setup or completion rules in this document.
