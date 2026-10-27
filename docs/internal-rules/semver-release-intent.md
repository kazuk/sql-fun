# SemVer release intent and Cargo feature lifecycle

Status: **final, approved policy** for #355, incorporating the reviewed #356
capability findings. This is the repository's release-intent classification
contract for implementation in #357–#361.

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

## Conservative comparison rule

The additional [capability investigation](semver-analyzer-probe.md) verifies
**cargo-public-api 0.52.0** against the same format-61 rustdoc JSON pairs used by
cargo-semver-checks 0.51.0, including function-like, derive and attribute macros.
Apply the following repository policy. Comparator and CI implementation remain
separate work:

1. After successful analysis of all intended coverage, no structural difference
   starts at Patch. Added-only public API starts at Minor. Any removed/changed
   item, or successfully observed API difference that cannot be classified,
   starts at Major.
2. Take the maximum of that conservative level and applicable SemVer findings,
   including warning-level findings.
   Added required trait methods and exhaustive enum variants demonstrate why
   additive-only diff cannot independently establish Minor. A reordered
   `PartialOrd` struct produces no public-api diff and exits 0, but its Major
   warning raises the combined result to Major. Exit codes alone are
   insufficient; an adapter unable to preserve finding levels fails validation.
3. Compare released Cargo feature inventories separately: removed released
   features require Major even with no API diff. New opt-in feature coverage
   must not suppress any released selected coverage. New APIs in that coverage
   require at least Minor under this policy.
4. Validate the complete old/new coverage, tool versions, compiler/target and
   JSON producer/schema provenance explicitly. A missing, unreadable,
   incompatible or failed artifact/tool is validation failure, not a SemVer
   verdict. Do not classify failures as Major or treat them as passes.
5. Review/test behavior outside structural JSON, including macro expansion and
   accepted macro input. An empty diff does not prove those contracts unchanged;
   a known unresolved concern remains inconclusive until evaluated.

This rule conservatively assigns Major to the measured harmless generic
parameter rename. That is a repository gate requirement, not a claim that Cargo
defines the change as breaking. The concrete-type versus `Self` control is
outside project inputs: the workspace's enforced `use_self` lint already
requires `Self` on both sides wherever applicable, so that spelling transition
does not arise in repository comparisons. It is not an unresolved policy case
and does not require normalization or an exception. Public value-parameter
renames are omitted by the tested public-api flags; moving private implementation
modules while preserving a re-export produces no diff.

The tested public-api diff distinguishes added/changed/removed public items and
includes auto/blanket/derived impls. Do not adopt simplification flags that hide
those surfaces without separately establishing their coverage. Its ordinary
CLI exits 0 for differences; `--deny` exits 1 for denied differences, but read
and parse failures also exit 1. Successful completion must be established before
interpreting categories; a raw exit-1 result is not a SemVer classification.

### Opt-in and JSON tooling boundaries

The shipped SemVer inventory contains 31 allow-by-default Minor lints, including
struct/enum/union additions, constants, statics and inherent methods. Enabling
them with `deny` detects the tested additions, but standalone functions, type
aliases, new traits and macros remain uncovered. Upstream #949 includes proposed
work and does not establish that those missing lints ship in version 0.51.0.

Direct SemVer JSON input also ignores manifest lint overrides: even passing the
candidate manifest did not enable the opt-in struct-addition finding. Do not
assume opt-in configuration is active in a JSON-only adapter. The public-api
diff supplies structural addition coverage independently; the JSON SemVer
default findings can still raise its conservative level.

Both tools accepted the exact same format-61 artifacts in the measured cases.
Format-55 inputs failed. Mutating only the format marker to 999 was accepted by
public-api but rejected by SemVer, so provenance and schema checks cannot rely
on public-api alone. Persist the producer details with artifacts and re-probe
when they change; reuse of arbitrary archived JSON is not established here.
The production JSON generation strategy and tool provisioning remain later
implementation decisions, with no new contributor procedure authorized.

The conservative over-classification tradeoff, complete released-coverage
requirement and complementary behavioral review boundary above are approved
repository policy. The measured combination establishes the documented cases,
not complete compatibility coverage of all Rust behavior. This specification
does not implement a comparator, CI wiring, baseline migration or fallback.

## Acceptance examples

The following cases are requirements of the comparison contract, not
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
| 8. Analyzer cannot classify a change required by this policy | Classify an observed unclassifiable API diff as Major; tool/analysis failure fails validation; out-of-model concerns remain inconclusive pending complementary review. No unsupported omission proves a pass. |

## Specification and implementation boundary

This approved policy and the reviewed #356 capability findings fulfill #355's
specification requirements. The capability report remains investigation
evidence. Package materialization, baseline storage/adoption, caching, CI wiring
and comparator implementation belong to #357–#361; their implementation is not
completed by this documentation change.
