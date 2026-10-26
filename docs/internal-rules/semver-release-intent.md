# SemVer release intent and Cargo feature lifecycle

Status: **provisional policy** for #355. The analyzer capability probe in #356 must be incorporated and reviewed before this becomes the final implementation contract.

## Authority and scope

Follow Rust/Cargo's SemVer compatibility guidance for public Rust APIs and Cargo features; do not invent a sql-fun-specific definition of a breaking API change. This document defines only how the repository applies those compatibility rules to release intent and feature-set coverage.

References:
- [Cargo SemVer compatibility guide](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo features reference](https://doc.rust-lang.org/cargo/reference/features.html)
- [Release intent parent issue #297](https://github.com/kazuk/sql-fun/issues/297)

A SemVer compatibility result is not equivalent to a build/test result. The analysis must use public APIs of the packages intended for publication, including APIs exposed by selected Cargo features.

## Release intent

Each publishable workspace crate has a `workspace.metadata.next-release` declaration of `patch`, `minor`, or `major` (see #297). It is the **maximum permitted required change level** for the upcoming stable release, not a requirement to introduce a change of that level.

The ordering is `patch < minor < major`. A candidate passes the release-intent gate only when the required change level is at or below its declared level. For example, `minor` intent accepts patch and minor changes, but rejects a major change; `major` accepts all three. Changes are classified using Cargo's compatibility guidance, and the most restrictive result across covered public API surfaces determines the required level.

This contract is defined for stable-to-stable compatibility. Version-numbering details for pre-1.0 crates and prerelease identifiers follow Cargo's SemVer guidance rather than a separate sql-fun policy; the release implementation must apply the appropriate baseline/version interpretation.

## Cargo features and coverage

A feature's existence and the public API exposed when it is enabled are distinct concerns:

- Adding an opt-in Cargo feature is additive coverage. An unreleased new feature has no historical public API contract of its own.
- Previously released features and the APIs previously available under released feature selections remain subject to compatibility validation, including when a selected configuration gains new features.
- A named product/feature-set configuration represents a set of APIs to validate, **not an immutable historical snapshot of feature names**. A configuration may acquire new feature coverage without turning that addition into an API break.
- Removing an already released feature, or breaking/removing APIs made available through released feature coverage, requires the applicable incompatible-change classification. Addition and removal are deliberately asymmetric.
- After a stable release adopts a new feature and its public API into the active baseline, subsequent comparisons must check that coverage as an existing contract.

### Comparing old and new feature selections

Compare the published baseline API with the candidate API while keeping previously released coverage visible on both sides. For a feature that exists only on the candidate side:

1. Evaluate previously released feature/API coverage against its corresponding candidate coverage; it must not be hidden by the newly added feature.
2. Evaluate the newly introduced feature's surface as **new coverage** with no baseline requirement for the new portion.
3. Do not require the baseline to enable a feature that did not exist then; do not declare an incompatibility merely because the old and new feature-name lists differ.

A configuration that is entirely new on the candidate side still cannot suppress checks for APIs previously released under other covered configurations. The exact selection and comparison mechanics belong to #358; this policy defines their expected outcome.

## Analyzer limitations and failures

`cargo-semver-checks` is an implementation candidate, not the authority for this policy. In particular, an analyzer reporting no violations does not automatically prove that additive API changes were fully classified. #356 must document each relied-on detection/classification capability (including proc-macro and feature-gated APIs), and distinguish an analysis failure from a genuine compatible result.

**Pending #356:** choose and document a concrete, deterministic outcome for each unsupported classification, especially additive public API changes (e.g. supplementary repository checks, explicit unknown/inconclusive results, or another justified mechanism). Do not silently interpret unsupported or failed analysis as a successful SemVer proof. The final policy must settle this before #357–#361 proceed.

## Acceptance examples

The following cases are requirements for the final comparison contract, not claims about current tooling:

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

#355 is specification-only. After #356 is completed, update this document with the tested analyzer version, observed gaps, and approved deterministic handling of all unsupported classifications. Final approval of that revision completes #355. Package materialization, baseline storage/adoption, caching, CI wiring, and implementation live in #357–#361 and are not defined here.
