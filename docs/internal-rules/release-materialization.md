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
Project tag numbers never select crate versions. A stable crate version applies
its release intent to select the next core; a SemVer pre-release retains its
current core and only changes or removes its suffix. Snapshot suffixes are
SemVer 2.0.0 pre-release values: dot-separated, non-empty ASCII
alphanumeric-or-hyphen identifiers, with no leading zero in a digits-only
identifier. Build metadata is unsupported. Non-publishable package versions
stay unchanged; their local dependency requirements may be updated. This step
does not consume/reset release intent or advance a SemVer baseline.

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

## Failure and current limitation

Any identity inconsistency, materializer failure, or unexpected changed path
fails the step and stops later release operations. A failure during manifest
updates may leave a partial runner-local worktree; discard it and rerun from
the original exact target. Do not consume partial output as successful state.

The materializer accepts stable and SemVer pre-release current crate versions.
It does not infer a pending version or implement a second lifecycle model.
