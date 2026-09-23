# CI custom image for GitHub actions for `sql-fun`

Run `ci/github-actions/check` with `GIT_ROOT` set to the repository root to
build the image and open an interactive container. After building, it prints
the uncompressed local image size in MiB and bytes, plus the size change when
an existing `ci-builder:local` image was available before the build. This is
not the compressed registry download size or the build cache disk usage.

- `--build-only`: build and report size without creating volumes or starting a
  container.
- `--rebuild`: pull the base image and build without the layer cache.
- `--clean`: remove the target volume before starting the container; ignored
  with `--build-only`.

To compare sizes, run `check --build-only` before and after editing the
Dockerfile. Use `docker history ci-builder:local` to inspect individual layer
sizes.
