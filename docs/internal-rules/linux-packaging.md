# Linux packaging

This workflow is for contributors building distributions and maintainers preparing
artifacts for installation tests. It produces one product containing `sql-fun`
and `sql-fun-server`, with the CLI crate owning the Debian metadata and version.

## Prerequisites

Use the repository development environment described in
[CONTRIBUTING.md](../../CONTRIBUTING.md). The standard CI image supplies the
packaging tools. Outside that image, use Ubuntu 24.04 with Rust/Cargo and install:

```sh
sudo apt-get install build-essential pkg-config libssl-dev \
  dpkg-dev binutils jq tar gzip coreutils
cargo install --locked cargo-deb
rustup target add x86_64-unknown-linux-gnu
```

`dpkg-dev` supplies shared-library dependency resolution; `binutils` supplies ELF
inspection and stripping tools. Packaging uses Bash, Git, standard Unix file tools,
and `sha256sum`. Docker access and root privileges are not needed to build packages.

## Build and inspect

From the repository, run:

```sh
.local/bin/check-test-flow --step=packaging
```

This optional step is listed by `check-test-flow --list-steps` and `--help`, but
is excluded from the default check flow. It calls `package-linux` for
`x86_64-unknown-linux-gnu`, which always validates the generated artifacts before
moving them to the output directory. There is no separate validation opt-in.

The only supported target is currently `x86_64-unknown-linux-gnu` (`amd64` in
Debian). Unsupported targets fail explicitly. Both executables are built together
with locked dependencies, default features, and the release profile. Stripping
happens once during the build; both archive formats contain identical executables.
No crate versions are changed. Ordinary CI packaging uses CLI SemVer filenames.
Release preparation supplies an explicit project tag to the packaging script;
all Release asset filenames use that tag while executable and package metadata
versions remain crate versions. For example,
`sql-fun_v0.1.0-snapshot4_amd64.deb` can contain Debian version `0.2.0~snapshot4`.
Only the internal Debian version uses `~` in place of the CLI SemVer prerelease
separator; no additional Debian revision is appended.

Outputs are under Cargo's target directory (including `CARGO_TARGET_DIR` overrides),
in `distributions/<target>/`:

- `sql-fun_<version>_amd64.deb`: executables in `/usr/bin` and license information
  in `/usr/share/doc/sql-fun`.
- `sql-fun-<version>-x86_64-unknown-linux-gnu.tar.gz`: `sql-fun`, `sql-fun-server`,
  and `LICENSE` at archive root.
- `SHA256SUMS`: SHA-256 checksums for the two distribution files.

Only successfully validated artifacts are published to the output directory.
The script checks package metadata, executable modes and architecture, checksums,
and byte equality between the build outputs and both extracted distributions.
To repeat that validation for version 0.1.0 with the default target directory:

```sh
.local/bin/check-linux-artifacts \
  target/distributions/x86_64-unknown-linux-gnu \
  x86_64-unknown-linux-gnu 0.1.0 \
  target/x86_64-unknown-linux-gnu/release
```

The Debian runtime library dependencies are calculated from both executables.
Cargo-deb excludes libgcc from its automatic results, so metadata explicitly
includes `libgcc-s1 (>= 4.2)`, as required by `dpkg-shlibdeps` on these binaries.
Build-image libraries are not automatically product dependencies. Rust/Cargo and
`cargo-expand` remain command-specific prerequisites for `sql-fun check`;
`cargo-expand` is not bundled or declared in Debian `Depends`.

## CI and support boundary

The `packaging` job in `build-test.yml` uses the same
`check-test-flow --step=packaging` entry point and uploads the two
archives and checksum manifest as `sql-fun-x86_64-unknown-linux-gnu`. Download that
workflow artifact for later installation tests; no GitHub Release is published.
The job logs the source commit, product version, target, and resolved dependencies.

These glibc-linked artifacts use Ubuntu 24.04 as their initial build and validation
baseline. Compatibility with older distributions is not promised. Packaging does
not install the Debian package, start the server, test PostgreSQL bootstrap, or
implement an installer. Those are separate follow-up tasks.
