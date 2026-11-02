# Release version materialization

## Source identity and responsibilities

`S` is the explicitly selected release-branch source with successful exact-SHA
required normal CI. `R` is a direct child recording the materialized release
state on that same branch. Preparation never writes to `main`.

The shared project parser supplies product tag, classification and snapshot
suffix. `.local/sbin/materialize-release-state` maps those explicit values to
`.local/bin/materialize-release-versions`; it does not introduce another mode
or version algorithm. Each publishable crate uses its own current version and
`workspace.metadata.next-release`, not the numeric product tag. Workspace
non-publishable versions stay unchanged; dependency requirements can change.

Responsibility boundaries:

- #325 materializes versions once in the exact-`S` checkout.
- #329 invokes the existing selected-candidate baseline adopter.
- #330 resets stable intent, explicitly stages owned state, commits/pushes `R`
  and resolves recorded state on reruns.
- #331's planned integration builds/validates release packages from committed
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

After materialization/manual preparation, the existing baseline operation
receives the candidate from the single selected CI run. Downloaded inputs stay
under `$RUNNER_TEMP/release-inputs`; the active baseline path alone is Git state.
`steps.baseline.outputs.paths` names external `baseline-paths.txt`: exactly
`.local/semver_baseline.tar.gz` for stable, empty for prerelease. The adopter
owns archive validation and atomic replacement; snapshots preserve baseline
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
mutations. The current workflow still selects CI for recorded `S` and adopts
reusable inputs/manual; #331 must bypass the whole producer path when a
matching immutable tag exists.

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
