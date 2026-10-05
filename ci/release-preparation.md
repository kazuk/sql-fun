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
rebuilding or repeating producer validation. This stage does not publish,
create tags, materialize crate versions, or require a future SemVer input.

## Maintainer tooling and permissions

The workflow uses Bash, Git, GitHub CLI (`gh`), and `jq` on Ubuntu runners.
For live local API diagnostics, install [GitHub CLI](https://cli.github.com/),
authenticate with `gh auth login`, and provide repository Actions read access.
The workflow authenticates using its GitHub token with `contents: read`
and `actions: read`; no write or publication credentials are needed.

Offline preparation contract tests are included in
`check-test-flow --step=version-materialization`. See
[contributor workflow validation](../CONTRIBUTING.md#release-workflow-validation)
for optional workflow linting.
