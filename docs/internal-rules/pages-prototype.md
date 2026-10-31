# Pages manual prototype

This guide is for maintainers validating the Pages consumer independently of
production releases (#299). The prototype publishes the existing manual from an
explicitly selected `build-test.yml` run. It does not build mdBook, create a
GitHub Release, publish crates, or invoke the production release workflow.
It replaces the repository's Pages site with that manual.

## Setup

Select **GitHub Actions** as the repository's Pages publishing source. The
`github-pages` environment must allow the ref selected for dispatch. Preserve
existing protection rules and satisfy environment approvals if configured.
The deployment job requires `pages: write` and `id-token: write`; the input
adoption job requires `actions: read` and checkout requires `contents: read`.
The Ubuntu runner supplies Bash, GitHub CLI, jq, tar, curl, and cmp. There are
no new CLI runtime or contributor prerequisites.

A new manual-only workflow on a PR branch can return HTTP 404 until GitHub has
registered it. The normal introduction path is through the default branch;
initial pre-merge registration for #299 used a separately approved temporary
push trigger on a dedicated ref. That trigger is absent from the final workflow.
Once available, select the branch or tag containing the prototype revision to
test. See GitHub's
[manual workflow trigger rules](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_dispatch).

## Run and verify

Choose both required inputs explicitly:

- `source_run_id`: a run of this repository's `build-test.yml` whose `doc` job
  succeeded and whose `sql-fun-user-manual` artifact is still available.
- `test_tag`: a stable-form test identity such as `v0.2.0`. This names the test
  archive; it does not claim that the artifact belongs to a real product release.

Dispatch **Pages manual prototype** (`pages-prototype.yml`) with those inputs
and the prototype ref to test. The workflow downloads only the selected run's
artifact and packages its existing files as
`sql-fun_vX.Y.Z_user-manual.tar.gz`, with `index.html` at archive root, following
the archive contract in #328. Its preparation job summary records the producer run,
source SHA, artifact name, archive name, and prototype revision/attempt. The
archive remains available as `prototype-manual-<attempt>` in that run's artifacts.

The deployment job extracts that archive, uploads the site through
`upload-pages-artifact`, and deploys with `deploy-pages`. It exposes the resulting
Pages URL and compares the served index and one nested CSS asset byte for byte
with the extracted manual. Its summary records the verified paths and URLs.
Record the run URL, tested revision, environment, and deployment URL in the PR
or issue as live validation evidence. Offline checks alone do not prove Pages
permissions or environment configuration.

Missing/expired artifacts, an unsuccessful manual producer, a missing archive,
or missing root `index.html` fail the run without selecting another input or
rebuilding content. An older producer run can still be selected explicitly;
the prototype does not apply production's latest-stable-release selection.

## Retry and completion

Correct the reported prerequisite and rerun the workflow, or dispatch again
with explicit replacement inputs if the selected artifact has expired. A full
rerun produces attempt-specific artifact names. Retry the complete workflow
rather than only a failed job so its preparation and deployment share the same
attempt. Runs serialize with the existing Pages deployment lock and do not
cancel an active deployment.

The prototype's offline contracts are excluded from standard local checks and
CI because no recurring execution budget is approved. Live deployment must
succeed at the final tested revision. Remove disposable validation refs when no
longer needed; retain run links as evidence. A successful prototype does not
establish that the complete production release chain has been exercised.
