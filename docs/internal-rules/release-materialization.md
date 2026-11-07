# Release version materialization

## Source identity and responsibilities

`S` is the explicitly selected release-branch source with successful exact-SHA
required normal CI. `R` is a direct child recording the materialized release
state on that same branch. Preparation never writes to `main`.

The shared project parser supplies the exact product tag, `stable|prerelease`
classification and opaque prerelease name. The name follows SemVer prerelease
syntax with the project's additional prohibition on hyphens; classification
reflects only whether the name is present. `.local/sbin/materialize-release-state`
checks those values against the explicit product tag and maps them to
`.local/bin/materialize-release-versions --release` or `--prerelease=NAME`.
The materializer validates all resulting versions with the same project parser
before changing any manifests and appends the name unchanged; it does not
interpret a channel or counter or introduce another version algorithm. Each
publishable crate uses its own current version and `workspace.metadata.next-release`,
not the numeric product tag. Workspace
non-publishable versions stay unchanged; dependency requirements can change.

Responsibility boundaries:

- #325 materializes versions once in the exact-`S` checkout.
- #329 invokes the existing selected-candidate baseline adopter.
- #330 resets stable intent, explicitly stages owned state, commits/pushes `R`
  and resolves recorded state on reruns.
- #331 builds/validates release packages from committed
  `R`, retains prepared assets and tags `R` only after success.
- #259's planned integration creates a draft after tagging and attaches assets.
  A maintainer reviews and publishes it.
- #260 publishes exact tagged crate versions. Stable-only full-branch merge
  into main remains downstream integration work; stable Pages uses the existing
  manual. Prereleases never merge into main or update Pages.

Normal-CI packages from `S` verify that source; they are not release packages
for materialized versions at `R`.

## Explicit paths and stable state

Materialization exposes external `versions` and `paths` files under
`$RUNNER_TEMP/release-state`: `crates.tsv` and `changed-paths.txt`. The latter
contains only workspace manifests and `Cargo.lock`, including non-publishable
member dependency edits from cargo-edit. The adapter rejects other tracked
changes and requires an initially clean tracked worktree/index.

The adapter also invokes `.local/sbin/release-binstall materialize` after version
materialization and checks the result before handing off paths. Both binary
crate manifests receive target-specific cargo-binstall metadata for
`x86_64-unknown-linux-gnu`: a concrete product tag in the download URL and producer
archive filename, `pkg-fmt = "tgz"`, and `bin-dir = "{ bin }"`. The helper consumes
`release-assets`' archive inventory and preserves unrelated manifest values.
These manifest changes enter `R` through the existing owned-path handoff, before
builds or source packaging. Recorded-state reuse never regenerates metadata;
old immutable releases are not rewritten to add support.

Every publishable crate is currently bumped on each new materialization, even
when its source is unchanged. Each newly published binary crate version is tied
to the product release that materialized it. Exact versions already on crates.io
retain their original published manifest and immutable archive URL; later
product releases and partial-publication retries skip them. Selectively reusing
unchanged versions would require preserving that original mapping; this feature
does not introduce such a release policy.

After materialization/manual preparation, the existing baseline operation
receives the candidate from the single selected CI run. Downloaded inputs stay
under `$RUNNER_TEMP/release-inputs`; the active baseline path alone is Git state.
`steps.baseline.outputs.paths` names external `baseline-paths.txt`: exactly
`.local/semver_baseline.tar.gz` for stable, empty for prerelease. The adopter
owns archive validation and atomic replacement; prereleases preserve baseline
bytes and modification time.

`.local/sbin/release-state commit` combines those explicit lists. Stable intent
reset also owns root `Cargo.toml` even when its path is absent from the
materializer list. After calculating versions, reset existing publishable crate
entries to `patch`, preserving unrelated metadata and comments. The repository
uses an explicit `[workspace.metadata.next-release]` table with scalar entries;
an unsupported representation fails without writing the reset. TOML parsing
checks the intended semantic change before writing. Prereleases preserve intent
and baseline. Unexpected tracked/index changes fail, rather than broad staging.
Untracked incidental files, external inputs, handoffs and public assets never
enter the commit. No `git add -A` is used.

## Commit and rerun handoff

`.local/sbin/release-state resolve` runs before producer mutation. It fetches
only the explicitly selected release ref and returns `source`, `revision` and
`status` (`new` or `reused`). A fresh attempt continues the original CI gate;
a recorded attempt checks out `R` and bypasses materialization/adoption/reset
mutations. Before CI lookup, `release-state tag-status` inspects the exact
remote tag.
A matching tag must resolve to validated recorded `R`; it bypasses the whole
producer path. With no tag, the workflow still selects CI for original `S` and
adopts reusable inputs/manual before building from `R`. Remote-query errors
are failures, never absence. Annotated tags are compared by peeled commit.

The release commit has subject `chore(release): materialize <product-tag>` and
unique trailers `Release-Source: <S>` and `Release-Tag: <product-tag>`. Its single
parent is `S`. These fields serve rerun identity, not independent artifact
provenance. Validation checks matching identity, direct ancestry, owned changed
paths and stable/prerelease intent semantics. A moved branch without the exact
matching record is a conflict. Do not guess identity from a commit subject alone.

Finalization requires an empty index, HEAD at `S` and the remote release tip
still at `S`. It records `R` with explicit GitHub Actions bot identity and pushes
normally to that exact release ref. An intervening incompatible update or push
rejection fails; no force, reset, amend or conflict recovery. If push transport
reports failure, accept only a subsequent fetch confirming exact `R` remotely.
Job-scoped `contents: write` on `GITHUB_TOKEN` supplies release-ref access and
avoids recursive push-triggered preparation. No main bypass actor is needed.

A rerun of the original source run can find remote child `R`; a run explicitly
selected at `R` recognizes the same record. Both reuse `R` without recalculating
versions. Other remote revisions or malformed records fail. Outputs distinguish
original `S` from committed `R`; `materialized-revision` and the final Cargo-derived
version TSV hand off the committed checkout to later build steps. Do not
re-check out `S` or rematerialize to reconstruct later job state.

## Failures and history

Before a successful commit/push, a failed mutation may leave partial runner
state; do not consume it as a successful handoff. Once `R` is recorded, preserve
it even if downstream build, tag, draft or distribution fails. Stable baseline
and reset intent remain on that branch, reaching main only through a published
stable full-branch merge. Preparation and all prerelease processing leave main
unchanged. No pre-publication patch back-commit is permitted.

Recovery need not succeed for every failure. Reuse recorded state when safe;
otherwise fail and let the operator explicitly abandon that version for a new
version/tag. Never delete/move tags, reset branches, force-update, erase release
commits or rewrite failed-attempt history. Do not delete recorded release
branches as routine producer cleanup.

A newly selected source with current prerelease crate versions is still subject
to #364's existing materializer limitation. Recorded-state reuse bypasses a
second bump; it does not introduce a prerelease lifecycle workaround.

## Prepared assets and tag handoff

`release-assets build` uses the committed checkout at `R`, a fresh external Cargo
target directory, and the already packaged manual. `package-linux` owns the
release build and package checks; assembly copies only the expected validated
outputs, renaming checksum metadata without changing its bytes. All public
filenames use the exact project tag. `release-assets` passes it explicitly to
`package-linux`; package metadata and executable versions still come from the
materialized crates. The complete
external directory contains exactly four nonempty public files; internal
baseline/state data is excluded.

The complete directory is uploaded before `release-state tag-push` runs. That
operation revalidates recorded `R`, checks the remote again, creates only the
selected local tag, pushes only its ref without force and confirms the remote
commit. A concurrent matching tag is accepted; a conflict or unconfirmed push
fails without rollback. Existing annotated and lightweight tags are recognized.

For an existing matching tag, `release-assets restore` locates artifacts using
Actions metadata for this repository's `release-prepare.yml` push runs on the
explicit release branch, whose head is original `S` or recorded `R`. Transport
names encode tag, `R`, run ID and attempt; no additional provenance manifest is
introduced. A run need not have finished successfully: retention precedes tag
push, which may have failed after the upload. The newest matching artifact is
selected; expiration, unavailable download or incomplete inventory fails without
falling back to different evidence or rebuilding. Restore checks availability,
not package validity. No producer validation is repeated.

`steps.assets.outputs` supplies `tag`, `source`, `revision`, `status`
(`produced`/`recovered`), `directory`, comma-separated `inventory`, `artifact-id`,
`artifact-name` and `run-id` to the same-job draft integration. Existing
job-level source/revision outputs retain their meanings. Paths are runner-local;
artifact locators provide cross-run recovery within ordinary retention. Missing
retained assets never authorize erasing the tag or rematerializing the version.

## Draft handoff and mutation boundary

`.local/sbin/prepare-release-draft` consumes the explicit `steps.assets`
tag/source/revision and external directory/inventory handoff, together with
the explicit workflow repository. The
shared project parser owns classification; `release-state tag-status` owns
record validation and remote-tag confirmation. No new identity record,
manifest, CI-run lookup or packaging strategy is introduced.

Authenticated paginated Release listings distinguish absent drafts from API
failures. After confirmation of the immutable remote tag, an absent Release
is created with an explicit REST `draft: true`, title/notes and parsed
prerelease status, immediately after confirming the existing remote tag.
New notes include exact publishable workspace crate versions from the existing
`release-crate-manifest` helper at `R`, so reviewers can distinguish product
identity from component versions.
Successful creation consumes the returned Release record directly: live
validation showed that an immediate list query can omit the new draft.
A failed/ambiguous create response is reconciled against the existing listing
without repeating the create operation. Existing drafts retain their metadata
and unrelated assets.
The helper lists assets by Release ID, requires existing expected assets to be
uploaded with the prepared size and SHA-256 digest, and uploads only missing
inventory names without `--clobber`. It re-reads state after upload responses and
failed creation responses whose server-side effects may already have completed.
The shared asset check hashes retained local files and rejects absent, unusable
or mismatched server digests during reconciliation, after upload and at final
completion. Incomplete or conflicting assets fail without replacement; producer
package validation is not repeated. A final inventory read confirms all required
draft assets.

Before each upload, the helper rechecks draft state and the remote tag. A
published Release is left untouched when observed. These checks do not lock
out a concurrent human publication; operators must wait for preparation to
finish and review the complete draft before publishing it. Partial drafts
and recorded Git state remain intact after failures. Summary output records
the URL/status and complete inventory on success, or the failure diagnostic.
Existing artifact locators remain in the earlier handoff summary.

The producer never publishes crates, merges into main or deploys Pages.
Reopened #260 owns publication-triggered crates → stable full-branch merge →
Pages ordering, protected-main access and downstream retry evidence. Producer
completion requires the complete draft and preserved branch/tag/asset handoff;
it does not depend on completing #260 or a live publication trial.

A full GitHub workflow rerun can remove artifacts from its previous attempt.
Live validation observed the initial complete-set artifact become unavailable
(artifact-ID lookup returned 404) after a full rerun; restore then stopped
before draft mutation. Do not assume ordinary retention protects artifacts
from rerun cleanup. Recovery still requires an available complete set from an
eligible run; missing bytes never authorize rebuilding after the immutable tag.
No retention-extension infrastructure or alternative producer is introduced.
