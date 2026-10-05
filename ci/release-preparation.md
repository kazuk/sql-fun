# Release preparation

Push an explicitly chosen source revision to a branch named
`release/vX.Y.Z`, `release/vX.Y.Z-snapshot`, or
`release/vX.Y.Z-snapshotN` (for example, `release/v0.2.0-snapshot1`).
Version components must not have leading zeroes. Normal releases originate
from an intended `main` revision; a hotfix may explicitly select a
maintenance/fix revision.

The pushed branch tip is the exact release target. The release preparation
workflow rejects invalid release branch names before adopting inputs. Tag
pushes do not trigger it. The shared project release parser determines stable
or snapshot classification; individual crate versions are a separate concern.

## Required normal CI

The exact target SHA must already have a successful required normal-CI run
of `build-test.yml`. This is a release eligibility gate: a missing successful
run means the target cannot be confirmed to have passed required validation.
The workflow fails before downloading inputs. A different SHA's success is
never substituted, even if its source appears equivalent.

Normal-CI triggers remain unchanged. Release preparation does not dispatch
or wait for another CI run, or rebuild selected artifacts to bypass validation.
After correcting repository/CI state, rerun preparation for the same SHA.
Alternatively, pushing a new explicitly selected SHA to the release branch
starts preparation for that new target, which must meet the same CI requirement.

## Adopted inputs

Preparation selects the newest successful eligible normal-CI run, ordered by
run creation time and then run number/ID. Rerunning an older run does not make
it a newly created run. Both required inputs come from the selected run:

- `sql-fun-x86_64-unknown-linux-gnu`
- `sql-fun-user-manual`

Missing or expired required inputs fail preparation. It does not fall back
to an older run. The job summary links the selected run and records the target
SHA and adopted inputs. Investigate the named producer/run when an input
cannot be downloaded, then retry with eligible CI state or a new target.

Binary/package and manual contents are adopted as produced by CI, without
rebuilding or repeating producer validation. Preparation does not publish,
create tags, push source changes, or require a future SemVer input.

## Materialized crate versions

After both CI inputs have been adopted, preparation materializes each
publishable workspace crate's version from its current version and
`workspace.metadata.next-release`. Ensure that every publishable crate has a
valid release-intent entry before selecting the release target. Project release
numbers do not determine crate versions. The parsed project identity selects
stable materialization or appends its snapshot suffix to each crate's next
version; no separate classification input is needed.

The job summary reports the materialized package versions. Updated manifests
and the lockfile remain in the same runner worktree for subsequent release
steps. Adopted CI inputs and version/path outputs stay outside that worktree;
this stage does not commit them. See the internal
[materialized-state handoff contract](../docs/internal-rules/release-materialization.md)
for integration with later release steps.

A materialization failure stops preparation. If a failure occurs after manifest
updates begin, discard the partial runner state and rerun from the original
exact target rather than reuse partial output. Currently, the materializer
requires stable current crate versions: a target already containing snapshot
versions fails. That existing limitation is tracked separately in #364.

## Maintainer tooling and permissions

The workflow runs on Ubuntu in the adopted pinned CI-builder container. It
provides Bash, Git, GitHub CLI (`gh`), `jq`, Cargo, `cargo set-version` (cargo-edit),
Taplo, and GNU `realpath` (coreutils); no separate runner tool installation is
required. For local contract tests, use the contributor setup linked below.

For live local API diagnostics, install [GitHub CLI](https://cli.github.com/),
authenticate with `gh auth login`, and provide repository Actions read access.
The workflow authenticates using its GitHub token with `contents: read`,
`actions: read`, and `packages: read`. Package read access authenticates the
CI-builder image pull from GHCR; no write or publication credentials are needed.

Offline preparation contract tests are included in
`check-test-flow --step=version-materialization`. See
[contributor workflow validation](../CONTRIBUTING.md#release-workflow-validation)
for optional workflow linting.
