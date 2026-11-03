# Release preparation

Choose an explicitly selected source revision `S` from `main` for a normal
release, or deliberately from a maintenance/fix branch for a hotfix. Push it to
`release/vX.Y.Z`, `release/vX.Y.Z-snapshot`, or
`release/vX.Y.Z-snapshotN` (for example, `release/v0.2.0-snapshot1`).
Version components must not have leading zeroes. The shared project parser
classifies stable versus snapshot; project release numbers and individual
crate versions are separate identities. Tag pushes do not start preparation.

## Release lifecycle and implementation status

| Stage | Stable release | Snapshot/prerelease | Status / owner |
| --- | --- | --- | --- |
| Select `S` | Successful required normal CI for the exact SHA | Same | Implemented, #326 |
| Materialize versions | Final crate versions from declared intent | Next crate versions with selected snapshot suffix | Implemented, #325 |
| Finalize release state | Adopt baseline and reset intent to `patch` | Preserve baseline and intent | Implemented, #329/#330 |
| Record source `R` | Commit and push release-state changes on the release branch | Same | Implemented, #330 |
| Release build and tag | Build/validate packages from `R`, retain assets, then tag `R` | Same | Implemented, #331 |
| Prepare draft | Create draft and attach completed assets | Same, marked prerelease | Implemented, #259 |
| Human publication | Review and publish the draft | Same | Maintainer gate |
| Distribution | Publish crates, merge the entire release branch into `main`, deploy existing manual | Publish crates; never merge into `main` or deploy Pages | #260; crates/Pages implemented, stable merge integration still required |

Preparation builds and validates distributable packages from recorded source
`R`, retains the complete prepared asset set, and creates and pushes the
immutable project tag at `R`, then creates or repairs a draft GitHub Release
with the complete public asset set. Normal-CI binaries from `S` are never distributed.

Review the draft and its completed assets before publishing it manually.
Publication authorizes #260 distribution.
Only a published **stable** release permits a history-preserving merge of the
**entire release branch** into `main`. Prereleases never merge into `main`.
There is no preparation-time main write or patch back-commit.

## Required normal CI and adopted inputs

The exact original source `S` must already have a successful required normal-CI
run of `build-test.yml`. Preparation neither dispatches nor waits for CI. It
selects the newest successful eligible run, ordered by creation time and then
run number/ID; rerunning an old run does not make it newly created. A different
SHA's result is never substituted, even if its source appears equivalent.

Both reusable inputs come from that single selected run:

- `sql-fun-user-manual`
- `sql-fun-semver-baseline` (internal `semver_baseline.tar.gz`)

Missing or expired inputs stop preparation before materialization; it never
falls back to an older run. The summary records the original SHA, selected
normal-CI link and inputs. Correct the reported eligibility/input problem and
rerun, or explicitly choose a new source with its own exact-SHA CI evidence.
The CI packaging artifact is verification-only and is not downloaded for
release preparation. Release packages require a new validated build from `R`.

## Stable and prerelease state

Before selecting `S`, ensure every publishable crate has a valid
`workspace.metadata.next-release` entry. Materialization uses each crate's
current version and declared bump, never the project tag's numeric version.
It updates workspace manifests/dependency requirements and the lockfile.

Stable preparation adopts the selected baseline into
`.local/semver_baseline.tar.gz`, then resets existing publishable crate intent
entries to `patch` **after** calculating release versions from their old values.
Prerelease preparation adds the explicitly selected suffix to next crate
versions and preserves both active baseline and intent. The adopter owns
baseline validation; the workflow does not build a replacement candidate.
The downloaded candidate stays internal and is never a Release asset.

Only intentional release-state files enter `R`, with explicit staging. CI
inputs, state handoffs and generated manual archives stay outside the checkout.
`R` is pushed to the originating release branch before the release
build/tag step. The summary reports its SHA and exact committed crate versions.
See the internal [state handoff contract](../docs/internal-rules/release-materialization.md).

The current materializer requires stable current crate versions. A newly
selected source already containing snapshot versions fails; issue #364 tracks that
separate lifecycle limitation. Reusing an existing recorded `R` does not rerun
the materializer or strip prerelease suffixes.

## Versioned manual

Preparation archives the adopted mdBook output as
`sql-fun_<project-tag>_user-manual.tar.gz`, preserving its files at the archive
root without rebuilding mdBook. Download it from the preparation run's
`release-manual-<attempt>` Actions artifact. Missing input, archive or upload
failure stops dependent preparation. The draft step attaches this archive
unchanged; stable Pages consumes that exact asset after publication.

The adopted manual is assembled from mdBook's HTML and Markdown renderers using
the same book source. The archive retains `index.html` at its root, together
with HTML navigation, styling, static assets and `highlighter/default.toml`.
Generated Markdown chapters retain their source-relative paths alongside the
HTML pages: for example, `diagnostics/function_not_found.md` and
`diagnostics/function_not_found.html`. Diagnostic Markdown paths follow
`diagnostics/<diagnostic_code>.md`. There is no enclosing `html/` or `markdown/`
directory in the archive, and Markdown files contain preprocessed Markdown text.

## Release packages and immutable tag

After `R` is committed and pushed, preparation builds the supported Linux target
`x86_64-unknown-linux-gnu` using the existing locked packaging command and its
artifact checks. A fresh external Cargo target directory prevents stale package
selection. Individual crate versions come from Cargo metadata at `R`; the CLI
crate owns the Debian metadata version, independently of the project tag.
All public asset filenames use the exact project tag, including its `v` prefix.
Release preparation supplies that tag to `package-linux`, which generates and
validates those filenames directly. Only the internal Debian package version
replaces the CLI prerelease `-` with `~`; `package-linux` owns that conversion.

The complete public asset set contains:

- `sql-fun-<project-tag>-x86_64-unknown-linux-gnu.tar.gz`
- `sql-fun_<project-tag>_amd64.deb`
- `sql-fun-<project-tag>-x86_64-unknown-linux-gnu-SHA256SUMS`
- `sql-fun_<project-tag>_user-manual.tar.gz`

Package filenames and bytes remain as validated. Only the checksum metadata
filename changes; its contents still name the original packages. The manual
archive is preserved unchanged. No SemVer baseline or internal state is public.

Download the complete directory from Actions artifact
`release-assets-<project-tag>-<R>-<run-id>-<attempt>`. The summary records its
name, artifact ID, producer run, `R` and inventory. Upload must succeed before
creating/pushing the tag. Build, validation, assembly or retention failure stops
tagging while preserving the recorded release branch. Successful local tag
creation alone is insufficient: the remote must confirm the exact `R`, including
when a push response is lost.

Existing remote tags are checked before CI input selection. A tag resolving to
the validated recorded `R` enters recovery; another commit or an invalid record
fails without mutation. Immediately before a new push, preparation rechecks for
concurrent tags and accepts only the same transaction. Existing tags are never
moved or overwritten.

Recovery uses only available complete-set artifacts from the release preparation
workflow for this release branch and original/recorded source. The most recently
uploaded matching set is selected; absent, expired or incomplete data fails
clearly without rebuilding, revalidation or extending retention. Ordinary
Actions retention applies. Draft creation/repair attaches the restored bytes
without rebuilding them. Retained assets and all generated build/assembly
output stay outside the checkout.

## Draft review and publication

The run summary links the draft and records the source revisions, retained
artifact locator and required assets. New drafts use the project tag as their
title and contain source information plus a table of exact publishable workspace
crate versions from `R` for review; snapshot drafts are marked
prerelease. Preparation never publishes the draft.

Before publishing, review the notes, stable/prerelease classification, tag and
four required assets listed above. You can edit draft notes without losing
them on a preparation rerun. Publishing authorizes distribution under #260:
crates.io publication, then the stable-only complete release-branch merge,
then stable-only deployment of the existing manual. The merge integration is
still tracked in reopened #260; a prepared draft does not establish that
post-publication distribution has completed.

A rerun preserves an existing draft's title, notes and unrelated assets, and
uploads only missing required filenames. It neither replaces nor deletes
existing assets. Each expected asset must be uploaded with the retained file
size and matching SHA-256 digest. A missing, unusable or mismatched digest,
conflicting size, incomplete upload or wrong prerelease classification stops
preparation for operator correction. An upload failure
may leave a partial draft; correct the reported problem and rerun using the
retained complete asset set. If that data is unavailable or expired, recovery
fails without rebuilding or changing the tag. A full workflow rerun can remove
artifacts from its previous attempt, even before their retention period ends;
do not treat a rerun as a guarantee that recovery data will survive. If no
complete retained set remains, preserve the failed version and explicitly
select a new release identity. Already published Releases are
left untouched; downstream failures are resumed through distribution, not
producer draft repair. A rerun may still fail earlier if its retained inputs
are unavailable.

## Safe reruns and failed attempts

A successful release-state push records `R` permanently. Rerunning the original
`S` run, or explicitly executing preparation on that recorded `R`, reuses the
same commit rather than bumping versions, resetting intent or adopting a
baseline again. Before tagging, CI evidence still belongs to original `S`;
preparation may re-adopt reusable inputs, recreate the manual and build again
from recorded `R`.
A matching remote tag skips CI selection/downloads, materialization, manual
packaging, baseline work, release builds and package validation entirely. It
restores the retained complete asset set for the same transaction instead.

A conflicting remote tip, invalid recorded state or rejected push fails safely.
If a connection fails after a server accepted the push, preparation confirms
success only when the remote contains the exact intended `R`. Main stays
unchanged throughout. After a later build or publication failure, stable
baseline/reset state remains on the release branch; it does not reach main
until successful stable publication and the full-branch merge.

Before a commit is recorded, discard partial runner-local edits after a failed
materialization/adoption and retry from the selected source. After `R` is
recorded, reuse that history; do not reconstruct it as a different commit.
Recovery is best-effort, bounded by available evidence and artifacts. Missing
or expired required data fails normally. An irrecoverable version may be
abandoned and a **new version/tag explicitly selected** for the next attempt.
Never delete/move tags, reset branches, force-update, erase commits or rewrite
history to conceal failed attempts. Preserve release branches for downstream
stable merge and recorded failed history.

## Maintainer tooling and permissions

The pinned CI-builder provides Bash, Git, gh, jq, Cargo/cargo-edit, Taplo,
uv/Python 3.12, cargo-deb, dpkg tools, binutils and GNU coreutils.
See [Linux packaging prerequisites][packaging-prerequisites].
No additional runner installation or contributor command is required. For local
API diagnostics use an authenticated GitHub CLI with repository Actions read
access.

The preparation job uses `GITHUB_TOKEN` with `contents: write` for release-branch
and immutable-tag pushes and draft/asset writes, `actions: read` for input
selection/downloads and
`packages: read` for the CI-builder pull. Its pushes do not trigger recursive
push workflows. Branch rules must permit normal release-ref and tag pushes;
main bypass is not required.
No registry or Pages publication credentials are used by preparation.

Release preparation contracts have no approved recurring execution budget;
they are not part of standard checks or CI. See the existing optional
[workflow validation procedure](../CONTRIBUTING.md#release-workflow-validation).

[packaging-prerequisites]: ../docs/internal-rules/linux-packaging.md#prerequisites
