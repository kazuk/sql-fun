# SemVer release intent and Cargo feature lifecycle

Status: **policy awaiting final review** for #355. The #356 capability findings
are incorporated below; the handling of unsupported classifications still
requires a maintainer decision before this becomes the final implementation
contract.

## Authority and scope

Follow Rust/Cargo's SemVer compatibility guidance for public Rust APIs and Cargo
features; do not invent a sql-fun-specific definition of a breaking API change.
This document defines only how the repository applies those compatibility rules
to release intent and feature-set coverage.

References:

- [Cargo SemVer compatibility
  guide](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo features
  reference](https://doc.rust-lang.org/cargo/reference/features.html)
- [Release intent parent issue
  #297](https://github.com/kazuk/sql-fun/issues/297)

A SemVer compatibility result is not equivalent to a build/test result. The
analysis must use public APIs of the packages intended for publication,
including APIs exposed by selected Cargo features.

## Release intent

Each publishable workspace crate has a `workspace.metadata.next-release`
declaration of `patch`, `minor`, or `major` (see #297). It is the **maximum
permitted required change level** for the upcoming stable release, not a
requirement to introduce a change of that level.

The ordering is `patch < minor < major`. A candidate passes the release-intent
gate only when the required change level is at or below its declared level. For
example, `minor` intent accepts patch and minor changes, but rejects a major
change; `major` accepts all three. Changes are classified using Cargo's
compatibility guidance, and the most restrictive result across covered public
API surfaces determines the required level.

This contract is defined for stable-to-stable compatibility. Version-numbering
details for pre-1.0 crates and prerelease identifiers follow Cargo's SemVer
guidance rather than a separate sql-fun policy; the release implementation must
apply the appropriate baseline/version interpretation.

## Cargo features and coverage

A feature's existence and the public API exposed when it is enabled are distinct
concerns:

- Adding an opt-in Cargo feature is additive coverage. An unreleased new feature
  has no historical public API contract of its own.
- Previously released features and the APIs previously available under released
  feature selections remain subject to compatibility validation, including when
  a selected configuration gains new features.
- A named product/feature-set configuration represents a set of APIs to
  validate, **not an immutable historical snapshot of feature names**. A
  configuration may acquire new feature coverage without turning that addition
  into an API break.
- Removing an already released feature, or breaking/removing APIs made available
  through released feature coverage, requires the applicable incompatible-change
  classification. Addition and removal are deliberately asymmetric.
- After a stable release adopts a new feature and its public API into the active
  baseline, subsequent comparisons must check that coverage as an existing
  contract.

### Comparing old and new feature selections

Compare the published baseline API with the candidate API while keeping
previously released coverage visible on both sides. For a feature that exists
only on the candidate side:

1. Evaluate previously released feature/API coverage against its corresponding
   candidate coverage; it must not be hidden by the newly added feature.
2. Evaluate the newly introduced feature's surface as **new coverage** with no
   baseline requirement for the new portion.
3. Do not require the baseline to enable a feature that did not exist then; do
   not declare an incompatibility merely because the old and new feature-name
   lists differ.

A configuration that is entirely new on the candidate side still cannot suppress
checks for APIs previously released under other covered configurations. The
exact selection and comparison mechanics belong to #358; this policy defines
their expected outcome.

## Analyzer limitations and failures

The [#356 capability report and reproducible fixtures](semver-analyzer-probe.md)
measure **cargo-semver-checks 0.51.0**, the version already pinned in the CI
image.
Use that version for authoritative comparisons until a replacement is probed.
Cargo's compatibility guidance remains the policy authority.

Observed capabilities and boundaries:

- Public function removal, parameter-count changes and adding `non_exhaustive`
  to an enum exceed patch/minor intent. Deprecation and `must_use` additions
  produce minor-level findings in the tested cases.
- Ordinary public function additions pass even with patch intent. Therefore
  the first successful patch/minor/major invocation is only a detected lower
  bound, not a complete required-level classifier.
- Feature-enabled API removal is detected only in selected coverage. Manifest
  feature removal is detected even when disabled. Candidate-only feature
  selections work, but a missing baseline feature supplied through the shared
  feature flag merely warns and succeeds; independently validate coverage.
  Bare JSON comparison misses the tested manifest feature removal, so a JSON
  adapter needs a separate feature-inventory comparison.
- Source-based selection excludes proc-macro-only crates with error 101.
  Direct rustdoc JSON comparison detects function-like, derive and attribute
  macro removals, but not the tested macro addition or expansion-body change.
  Later implementation must explicitly validate its JSON production path and
  retain macro behavior tests; a source-only run cannot claim macro coverage.
- Exit `100` means completed analysis found denied violations exceeding the
  requested level. Exit `101` means analysis failed, including compilation
  failure even for major intent. Treat other unexpected failures as incomplete
  analysis as well. Exit `0` alone does not establish complete classification
  or prove every intended surface was checked.

Unsupported classifications remain **unresolved/inconclusive**, never a proven
pass. This applies even when a detected lower bound fits the declared intent.
The findings do not authorize silently weakening the maximum-required-level
contract to a detected-breaking-change-only gate.

**Pending maintainer decision:** approve a concrete supplementary classification
or evidence protocol for unsupported cases, or explicitly revise the gate's
contract. No such mechanism is selected or implemented by this document. The
existing inconclusive outcome is deterministic, but does not complete the policy
requirement to settle how the release gate can establish all required levels.
Implementation issues #357–#361 must wait for that scope decision and final
policy approval.

## Acceptance examples

The following cases are requirements for the final comparison contract, not
claims about current tooling:

| Case | Expected policy outcome |
| --- | --- |
| 1. Add an unreleased, opt-in feature to an existing supported configuration | No compatibility violation solely from the feature's addition. |
| 2. Continue development of that new feature while old APIs are present | Continue checking all previously released API coverage. |
| 3. Remove a released feature or break its previously released API | Classify according to Cargo's incompatible-change rules; enforce release intent. |
| 4. Introduce a configuration whose feature set exists only in the candidate | New coverage has no historical contract; old released APIs present in that configuration remain checked. |
| 5. Adopt the new coverage into a stable active baseline, then break it in a subsequent candidate | Treat the now-released coverage as a normal compatibility contract. |
| 6. Add versus remove feature coverage | Addition is not treated as removal; removal of released coverage must not be treated as harmless addition. |
| 7. New side enables an opt-in feature absent from the baseline | Compare the common historical surface for compatibility and classify the candidate-only surface as new coverage, without requiring a nonexistent baseline feature. |
| 8. Analyzer cannot classify a change required by this policy | Report an explicitly unresolved/inconclusive classification until the final policy defines and validates a supplementary mechanism; never claim a proven pass from the analyzer's omission. |

## Finalization boundary

Issue #355 is specification-only. The #356 report supplies the tested version
and observed gaps. Final approval still requires an explicit decision on
unsupported classifications and review of that revision; incorporating
measurements alone does not complete #355 or #356. Package materialization,
baseline storage/adoption, caching, CI wiring, and implementation belong to
issues #357–#361 and are not defined here.
