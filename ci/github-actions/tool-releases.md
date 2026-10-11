# CI image tool releases

## Local builds and prerequisites

Local image builds require Docker CLI access to a running daemon, Bash, `curl`,
`jq`, and network access to GitHub release metadata and downloads. The standard
development image includes `curl` and `jq`; contributors building outside it
must install them alongside Docker.

`check` resolves the latest stable official cargo-binstall and typos releases
before building and prints the selected tags. The image supports Linux amd64.
Missing release assets or SHA-256 digests stop the build. Archives are checked
against the release asset digests before extraction; a checksum mismatch fails
installation without compiling either tool from source. Both installed versions
are checked after the local build, including with `--build-only`.

For a direct `docker build`, supply these required build arguments in addition
to `SERENA_TAG`:

| Argument | Value |
| --- | --- |
| `CARGO_BINSTALL_TAG` | Official cargo-binstall release tag, including `v` |
| `CARGO_BINSTALL_SHA256` | SHA-256 of `cargo-binstall-x86_64-unknown-linux-musl.tgz` |
| `TYPOS_TAG` | Official typos release tag, including `v` |
| `TYPOS_SHA256` | SHA-256 of `typos-<tag>-x86_64-unknown-linux-musl.tar.gz` |

Obtain the tag and matching asset digest from each official repository's
GitHub Releases API, as `check` does. Checksum arguments contain the 64 hexadecimal
characters of the asset digest without its `sha256:` prefix. The Dockerfile
consumes these inputs and does not discover tool versions. A published image's
immutable digest remains the downstream reproducibility boundary.

## Reviewing published versions

The local check and both image workflows resolve the latest stable official
cargo-binstall and typos releases before Docker builds. The PR image build
records the selected release tags in the GitHub Actions job summary.

The publisher resolves these releases once and passes identical versions and
checksums to its smoke-tested build and its publication build. Its generated
main adoption PR records both release tags alongside the image digest and source
commit. Review those versions when adopting the image. Separate PR and publish
runs may resolve different latest releases; the publish smoke tests validate
the versions recorded in the adoption PR.

Follow [the CI image maintenance process](../README.md) for publication and
separately reviewed digest adoption.

## Scrut for CLI contracts and E2E

The image includes the native Scrut v0.4.3 executable on `PATH`, matching the
installed-package validation jobs. Contributors can run Scrut-based
`check-test-flow` steps without installing another tool. Bash and `jq`, needed
by the CLI E2E runner, are already included in the image.

The Dockerfile verifies the SHA-256 of the pinned DotSlash v0.5.9 archive and
Scrut v0.4.3 wrapper before executing them. DotSlash verifies the downloaded
native executable against the wrapper's embedded digest. Only the native Scrut
binary is copied into the final image; DotSlash and its download cache are
build-time dependencies. The local check and both image workflows verify
`scrut --version` against `scrut 0.4.3`.

When changing this pin, review the corresponding official release downloads
and checksums and keep the image smoke checks aligned. Update the
installed-package validation jobs in the separately reviewed main adoption PR
if their Scrut pin needs to change.
