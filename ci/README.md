# CI Image Maintenance

The GitHub Actions CI environment is built from
`ci/github-actions/Dockerfile` and published to GHCR.

CI image changes use a dedicated `ci-image` branch. Changes to the image are
reviewed before publication, and the published image is adopted by `main`
through a separate pull request.

This keeps image publication and image adoption as distinct reviewable steps.

## Update Flow

```text
Dockerfile change
    |
    v
pull request -> ci-image
    |
    | CI image build and smoke test
    | exactly one commit required
    v
merge into ci-image
    |
    v
build and publish image to GHCR
    |
    v
create main-based adoption branch
    |
    | cherry-pick the ci-image commit
    | update the pinned image digest
    v
pull request -> main
    |
    | optional workflow changes using the new image
    | normal CI and human review
    v
merge into main
```

## Updating the CI Image

1. Create a branch for the CI image change.

2. Update `ci/github-actions/Dockerfile` and any files required to build the
   image.

3. Optionally run the local CI image check while developing:

   ```sh
   ci/github-actions/check
   ```

4. Keep the change as exactly one commit.

   Pull requests targeting `ci-image` are required to contain exactly one
   commit. The commit is later cherry-picked into the `main` adoption pull
   request, so one commit represents one CI image update.

5. Open a pull request targeting `ci-image`.

   The `ci-image` branch is protected and requires pull requests.

   `.github/workflows/ci-image-build.yml` runs for pull requests targeting
   `ci-image`. It verifies that:

   * the pull request contains exactly one commit;
   * the Docker image builds successfully; and
   * the image passes its smoke tests.

   The image build check is a required check for merging into `ci-image`.

6. Review and merge the pull request into `ci-image`.

   CI image changes affect the software supply chain used by all subsequent CI
   jobs. Review changes to downloaded tools, versions, checksums, package
   sources, build commands, and other executable content accordingly.

   The `ci-image` branch uses rebase merging so that the reviewed single commit
   becomes the commit published from `ci-image` without introducing a merge
   commit.

## Publishing the Image

A push to `ci-image` triggers
`.github/workflows/ci-image-publish.yml`.

The workflow:

1. builds the CI image;
2. runs the image smoke tests;
3. publishes the image to GHCR;
4. obtains the immutable image digest;
5. creates a new branch from `main`;
6. cherry-picks the `ci-image` commit onto that branch;
7. updates the pinned `ci-builder` digest in
   `.github/workflows/build-test.yml`; and
8. opens a pull request targeting `main`.

The generated branch name includes the first eight hexadecimal characters of
the published image digest:

```text
ci-image-<digest>
```

For example:

```text
ci-image-52907a6c
```

The generated pull request title is derived from the `ci-image` commit title:

```text
ci: image update for <original commit title>
```

## Adopting the Published Image

The automatically created pull request targeting `main` is the adoption point
for the newly published CI image.

The pull request contains both:

* the source change that produced the image, cherry-picked from `ci-image`; and
* the immutable GHCR digest that makes the main CI workflow use that image.

If the new image requires corresponding workflow changes, add those changes as
additional commits to the generated pull request branch before merging.

For example, adding a tool to the CI image and then using that tool from a
GitHub Actions workflow should normally be handled as:

```text
ci-image pull request
    Add the tool to the image.

main adoption pull request
    Cherry-picked image source change.
    Update the pinned image digest.
    Add or update the workflow that uses the tool.
```

Do not automatically merge the adoption pull request.

Switching the CI image changes the executable environment used by repository
CI and is therefore a supply-chain change. The final pull request must receive
normal human review before it is merged into `main`.

After the adoption pull request is merged, `main` contains both the source
needed to reproduce the active CI image and the immutable digest identifying
the exact image used by CI.

## Compilation timing reports

The regular `workspace-all-features` job uploads Cargo HTML timing reports as
`cargo-timings-workspace-all-features`, retained for 30 days. The upload runs
even if compilation or tests fail. A failure before Cargo generates a report
can leave no timing files; consult the failed step's log in that case.

When investigating CI performance, download the artifact from the selected
Actions run and open its HTML report. Compare long compilation units and
parallelism with earlier retained reports before they expire, recording the
run links, source revisions, toolchain, image digest, and cache state alongside
any conclusions. Periodically revisit these reports during CI maintenance to
identify regressions and newly dominant units.

The reports measure the existing nextest build, without an additional build
for telemetry. The job also performs feature-boundary checks and a workspace
`cargo check` before nextest; those phases and test execution contribute to
job duration separately. Build-script timings include more than native C
compilation, so a slow `libsqlite3-sys` unit alone does not establish SQLite C
compilation cost. Correlate reports with build logs and sccache statistics.
