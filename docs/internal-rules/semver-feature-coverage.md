# Released feature coverage

This is the internal #358 contract for release maintainers and #359/#360
implementers. Scripts coordinate the existing
[one-configuration comparator](package-api-comparator.md). The
[additive-feature rule](semver-release-intent.md#repository-operational-rule-additive-cargo-features)
is a repository requirement, not an additional analyzer.

## Explicit inputs

Invoke the internal script with absolute input/tool paths:

```sh
.local/sbin/compare-feature-coverage \
  --baseline /repo/.local/semver_baseline.tar.gz \
  --candidate /prepared/coverage.json \
  --policy /repo/.local/semver-feature-sets.toml \
  --diff-wrapper /repo/target/debug/xbuild \
  --semver-checks /tools/cargo-semver-checks \
  --toolchain stable
```

The selected installed toolchain and target govern current API production.
Released APIs retain their recorded producer information. The
comparator's Bash/rustup/tool prerequisites and analysis limits apply. The
lifecycle/seed scripts require Python 3.12 for archive extraction filters; the
maintained test harness provisions that interpreter through existing uv setup.
This operation does not install tools or discover releases. It uses the current
directory as its Cargo configuration/build context, including compiler wrappers,
flags and persistent build outputs; Cargo may download dependencies. Current
source packages and policy come from explicit candidate inputs.
The baseline contains generated resources, never historical build inputs.
Repository integration supplies the selected repository as the Cargo context.

Internal current-source preparation uses schema 1:

```json
{
  "schema": 1,
  "source_revision": "explicit-source-commit",
  "target": "x86_64-unknown-linux-gnu",
  "default_features": true,
  "packages": {"example": "packages/example-0.1.0"},
  "configurations": {"postgres": {"example": []}}
}
```

Package paths are relative to and contained within the descriptor directory.
Each path contains extracted, normalized Cargo package contents. The package
map is the complete publishable set selected from that source's Cargo metadata:
workspace membership, local source and `publish != []`, matching
`release-crate-manifest`. The producer owns that selection; the consumer
validates
identities, publishability and policy mappings against the supplied set.
Only the candidate's packaged dependency closure is resolved and built. Released
features, resolved coverage and API target kinds come from the saved baseline
metadata. The historical source/dependency graph is neither retained nor
rebuilt.

For the candidate, the explicit TOML policy supplies current configurations;
its descriptor's configurations are replaced by that policy. Released state
uses the configurations recorded in the active baseline. Every configuration
maps every package on its own side, including binary-only packages, to an
explicit string list of additional Cargo features. Defaults are enabled;
`[]` adds no features. Missing mappings, unknown/private packages, unknown
features, malformed values and empty policy are errors. Feature forwarding must
be selected explicitly per package. Names are enumerated without a hardcoded
product/crate list. The checked-in policy initially defines `postgres` and
`sqlite`, enabling each package's `sqlite` feature where present.

## Comparison and results

For every currently publishable package with historical coverage, compare its
released selection against that selection in the candidate. If the retained
product gains
features, also compare the historical selection against the expanded current
selection. Identical package/selection pairs execute once. A released selection
is not discarded when another product is added.

Candidate-only, never-released packages have no SemVer contract. Record them
with `reason: "new-package"` in `new_coverage` and retain them in
`next_coverage`, but do not schedule a self-comparison or require a bump solely
for their introduction. Unchanged released packages plus a new crate therefore
remain Patch. Build validation of new crates belongs to ordinary builds/tests; a
new crate can still be compiled as a dependency when checking a released crate.
After stable baseline adoption, compare its APIs as released coverage normally.

For a current-only product containing a released package, a new name selecting
an actual released selection reuses its historical comparison and is marked
`released_selection`; naming alone does not impose Minor impact. For genuinely
new selections of released packages, self-comparison validates the current
surface and reports new coverage with conservative Minor impact. It does not
establish a published predecessor for the selection. Historical comparisons
still run and can raise the overall result to Major. The additive-feature rule
makes special cross-product API attribution unnecessary. There is no
all-products comparison matrix or analyzer for features that hide existing APIs.

Every current library/proc-macro selection of an already released crate must be
validated, even when it shrinks or replaces historical features. If
historical/expanded calls already cover that selection, reuse them. Otherwise
run a `current-validation` call on the actual selected surface. Keep coverage
removal evidence, but never use Major removal evidence as a substitute for
successful analysis: a compile failure still produces `analysis-error` without
an impact or `next_coverage`.

Removed released manifest features, dropped product/mapping coverage and removed
library targets of checked packages contribute explicit Major evidence. Packages
that are now private/no longer publishable are excluded, even when the baseline
contains historical coverage; their removal does not contribute a SemVer
verdict.
Also retain lost resolved feature coverage from #357's old/new coverage
evidence,
even when no public API item changes. Saved resource metadata preserves released
defaults and resolved forwarding.
Detect missing released features before requesting an impossible current build.
A policy requesting an unknown current feature is an input
error. Binary-only packages remain explicit in policy but their reports exclude
Rust API analysis; this is not a CLI compatibility guarantee.

Success is exit 0 with JSON `status: "detected"`. `impact` is the maximum of
comparison results, new coverage of released packages (Minor) and coverage
removals (Major). Never-released package introduction contributes no bump.
`comparisons` retains package/product/kind and #357's complete evidence;
`new_coverage`, `removed_coverage`, `excluded`, `limits` and `next_coverage`
explain the result. Success does not mean the release intent permits it: #359
owns that decision. Failure is exit 1 with `status: "analysis-error"`,
diagnostics and limits, without an impact. Invalid command syntax uses argparse
exit 2.

`next_coverage` records internal current-source preparation state and is not an
adoptable baseline. The `check-semver --candidate-output` option archives the
complete current publishable set, including never-published crates, reusing
comparison JSON and
generating only missing current configured selections. Adoption grants
released
status only after #360 validates and installs those resources. Missing/corrupt
baseline input fails; it never bootstraps from the candidate.

## Resource baseline format

Schema 2 contains `source_revision`, `target`, default-feature policy, complete
`configurations` and a `packages` inventory. Each package records its version,
feature inventory, whether it has a Rust API, and generated `selections`. Each
selection contains its relative rustdoc JSON path, SHA-256 digest, coverage
metadata (selected/resolved features, target kind, identity and schema), and
producer information. Every configured library/proc-macro selection must have a
resource. Binary-only packages retain metadata without a JSON artifact.

Paths must remain within the bundle. Missing artifacts, inconsistent coverage,
corrupt digests and unsupported formats fail analysis. No historical Cargo
manifest, source package, registry dependency or compiler environment is
required.
Both consumers read JSON directly; no duplicate public-API text is needed.

## Initial active baseline

The shared active path is `.local/semver_baseline.tar.gz`. The archive contains
`baseline.json` and generated JSON under `rustdoc/`, with no source packages.
Links and special files are rejected. Schema 1 source archives are not accepted
as a baseline and never trigger a rebuild fallback.

The maintainer approved revision
`bd53b083b028dc8423f6c4e5b232cbd3c851c8c1`, recorded in the crates.io
`sql-fun 0.1.0` package. The maintainer confirmed that its `dirty` marker came
from publication-script work and approved this commit's API structure as the
seed. This is source-revision approval, not byte equality with the published
archive.

The initial state records default-enabled `postgres` coverage for that
revision's complete publishable set, including then-publishable tooling and
examples. SQLite coverage did not exist there. Current package additions and
previously publishable packages becoming private are excluded from SemVer
verdicts, not seed-specific exceptions.

`.local/sbin/seed-semver-baseline` produces this initial baseline once. It
requires explicit repository, output, target and toolchain arguments, exports
the
fixed approved revision, supplements its missing local dependency versions and
packages its sources. It generates JSON for that revision's configured API
selections, retaining only resources and metadata in the final archive. This
one-time migration/production operation is separate from normal validation.
Existing output files are never overwritten. It neither advances the source
revision nor adopts a release candidate. Producing resources requires a build
environment; consuming them never recreates that environment or rebuilds them.

### Observed analyzer limitation in full-seed replay

A manual full-seed self-comparison completed all nine library/proc-macro
comparisons. Eight were Patch, but cargo-semver-checks 0.51.0 reported
`trait_changed_kind` for `sql_fun_sqlast::syn::ListOpt` despite identical
source inputs and an empty structural diff. The historical source explicitly
re-exports the `ListOpt` trait alongside a glob that also contains a shadowed
`ListOpt` struct. This is evidence of an existing analyzer limitation, not a
lifecycle change or a failed build. The coordinator preserves that Major finding
under #355's policy; it does not suppress it, alter the approved seed or exclude
released SQLAST coverage. The independent false finding is tracked in
[#826](https://github.com/kazuk/sql-fun/issues/826) for a separate PR before the
enforcing #361 gate.

The corrected image-provided analyzer `0.51.0-sqlfun.1` makes all nine
comparisons Patch without changing lifecycle policy or suppressing findings.
The [#826 investigation](semver-shadowed-trait.md) records the cause,
image-owned correction and historical complete-baseline validation evidence.

## Package-set lifecycle and maintained verification

Compare the released resource inventory with the current publication set. A
candidate-only crate does
not require an old mapping/package, a self-comparison, a Minor bump or a manual
baseline rewrite. Continue released-crate comparisons in the same run. After
explicit stable adoption, new packages become historical obligations. A
historical
package absent from the current publication set is reported as an
explicit `no-longer-publishable` exclusion. A new package is explicitly
`never-published`; neither receives an API/intent verdict. Only current API
production requires a packaged dependency closure.
The complete current package map and `next_coverage`, including never-published
packages, remain available for #360 adoption. No rename inference is performed.

`xbuild/tests/package_api.rs` runs lightweight lifecycle Python contracts
through
the existing `workspace` check and nextest archive group. They verify
scheduling,
policy errors, package addition/adoption/exclusion, impact aggregation and error
propagation using small source packages and supplied comparator reports. They
also extract the checked-in seed and validate its complete replay mappings
without compiling historical sources. Replacement/shrink cases check that the
current selection is scheduled and its analysis failure propagates even when
removal already supplies Major evidence.

The #828 [test cost review](workspace-test-cost.md) discarded the costly real
compiler/analyzer comparison suite and duplicate historical-baseline fixture;
that coverage is not moved to a manual test path. The lifecycle suite remains
within the per-case limit and does not repeat isolated Cargo/rustdoc builds or
fetch historical dependencies. Real-source lifecycle acceptance checks performed
during implementation remain validation evidence, rather than recurring CI
tests.
Normal contributor commands and prerequisites remain unchanged.

Ongoing candidate generation, archive validation and stable/snapshot adoption
follow the [repository integration contract](semver-repository-integration.md#explicit-adoption).
The initial #358 baseline already exists; no initialization path is provided.
Issue #361 owns release CI wiring, and #329 owns release-side selection and
orchestration. The feature lifecycle itself does not implement release
transactions, optimize caches or expand the installed product CLI.

Repository source selection, package production and intent enforcement are
documented in [repository integration](semver-repository-integration.md).
