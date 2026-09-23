# CI Image Maintenance

The GitHub Actions CI environment is built from `ci/github-actions/Dockerfile` and published to GHCR. Updates to the CI image are intentionally reviewed in three stages: the Dockerfile change, the image publication, and the final digest switch used by the main CI workflow.

## Update Procedure

1. Update `ci/github-actions/Dockerfile`.

2. Run the local CI image check:

   ```sh
   ci/github-actions/check
   ```

   This builds the image locally. Verify that the tools or dependencies introduced by the Dockerfile change are available and work as intended inside the image.

3. Open a pull request targeting `main`.

   Review the intent of the Dockerfile change and verify that the implementation is correct. Merge the pull request into `main` after review.

4. Refresh remote-tracking references, create a new branch from `origin/main`, then open a pull request targeting the `ci-image` branch.

   ```sh
   git fetch origin
   git switch -c <branch-name> origin/main
   ```

   This pull request is the explicit review point for deciding whether the updated Dockerfile should be published as the CI image. Merge it into `ci-image` after review.

5. A push to `ci-image` triggers `.github/workflows/ci-image-publish.yml`.

   The workflow builds the Docker image and publishes it to GHCR.

   After the workflow completes, copy the published image digest from the `Show digest` step.

6. Refresh remote-tracking references and create a new branch from `origin/ci-image`.

   ```sh
   git fetch origin
   git switch -c <branch-name> origin/ci-image
   ```

   Update the `image` value of the `ci-builder-container` anchor in `.github/workflows/build-test.yml` to use the digest produced in the previous step.

   Open a pull request targeting `main`. This pull request is the final review point before switching the repository CI to the new image.

   Merge the pull request after confirming that the new image should become the active CI image.
