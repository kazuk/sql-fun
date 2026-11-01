# Release version materialization

Release preparation runs materialization in the existing `prepare` job, after
release identity parsing, successful exact-target-SHA normal CI selection, and
required CI-input adoption. The job uses the adopted pinned CI-builder image
for Cargo, cargo-edit, Taplo, Git, Bash, and jq.

## Identity and version ownership

`.local/sbin/materialize-release-state` is an internal workflow adapter. Its
classification and snapshot suffix are explicit outputs of the shared project
release parser used by release preparation and distribution. Stable identities
map to the existing materializer's `--release` operation; snapshot identities
map to `--snapshot=<parsed suffix>`. No second operator mode is introduced.

The existing materializer owns crate version calculations. Each publishable
crate uses its own current version and `workspace.metadata.next-release`.
Project tag numbers never select crate versions. Non-publishable package
versions stay unchanged; their local dependency requirements may be updated.
This step does not consume/reset release intent or advance a SemVer baseline.

## State handoff and Git boundary

The workflow checks out the exact release target, then materializes once in
that worktree. Subsequent producer steps must use that same worktree. Do not
check out the original revision again or rerun the version bump to reconstruct
state. The original target SHA remains the identity of the adopted normal-CI
inputs, independently of the eventual materialized source commit.

The materialization step exposes these step outputs for later same-job consumers:

- `versions`: `$RUNNER_TEMP/release-state/crates.tsv`, one package/version TSV
  row per publishable package, in package-name order.
- `paths`: `$RUNNER_TEMP/release-state/changed-paths.txt`, changed repository
  paths, one per line, restricted to workspace manifests and `Cargo.lock`.

Finalization (#330) owns staging and committing this state, together with any
other explicitly owned release-state updates. It must stage explicit paths;
never broadly stage the worktree. This step creates no intermediate commit and
makes no remote Git changes. A later job boundary must transport/pin the
materialized source state explicitly rather than recalculate versions.

CI inputs remain under `$RUNNER_TEMP/release-inputs`. Both that directory and
materialization outputs stay outside the Git worktree. The adapter requires a
clean tracked worktree/index and a new external state-output directory. It
rejects any materializer edit outside workspace manifests and the lockfile;
unrelated untracked files are not included in the changed-path output.

## Baseline state handoff

After materialization and manual preparation, the same `prepare` job invokes
`.local/sbin/adopt-semver-baseline` with the parsed classification and the
candidate downloaded from the already selected exact-SHA normal-CI run. The
candidate remains at
`$RUNNER_TEMP/release-inputs/sql-fun-semver-baseline/semver_baseline.tar.gz`.
Archive validation and stable/snapshot semantics belong to the existing
[adoption contract](semver-repository-integration.md#explicit-adoption).

After successful adoption, `steps.baseline.outputs.paths` names
`$RUNNER_TEMP/release-state/baseline-paths.txt`. For stable releases it contains
only `.local/semver_baseline.tar.gz`, followed by a newline; for snapshots it is
empty. A stable entry is intentional even when idempotent adoption leaves no
Git diff. `steps.baseline.outputs.status` carries the adopter's result for the
job summary. Failed adoption exposes no successful handoff.

Finalization (#330) must combine this separate explicit path list with
`steps.materialize.outputs.paths`, staging only intended repository state.
Materialization's manifest/lockfile allowlist is unchanged. Do not stage the
downloaded candidate, external path lists, or release assets. These outputs
serve subsequent steps in the same job; they neither transport state across
jobs nor commit/back-push it. Adoption errors stop dependent finalization;
a later failure requires discarding runner-local state and rerunning from
the original target rather than committing the baseline independently.

## Failure and current limitation

Any identity inconsistency, materializer failure, or unexpected changed path
fails the step and stops later release operations. A failure during manifest
updates may leave a partial runner-local worktree; discard it and rerun from
the original exact target. Do not consume partial output as successful state.

The materializer currently requires stable current crate versions. A target
whose manifests already contain snapshot versions cannot yet be materialized
again; prerelease-current-version lifecycle support is tracked separately in
issue #364. Preparation propagates this failure. It does not strip suffixes,
infer a pending version, or implement a second lifecycle model.
