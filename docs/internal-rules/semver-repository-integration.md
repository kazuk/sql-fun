# Repository release-intent integration

`check-semver` implements #359 for maintainers validating local changes and for
future CI adapters. It reuses the #357 comparator and #358 feature lifecycle;
see [policy](semver-release-intent.md), [coverage](semver-feature-coverage.md),
and [package comparison](package-api-comparator.md).

## Source selection and isolation

The standalone command requires an absolute repository root, one source mode,
an installed toolchain and an explicit target:

```sh
.local/bin/check-semver --repository "$PWD" --working-tree \
  --toolchain stable --target x86_64-unknown-linux-gnu
.local/bin/check-semver --repository "$PWD" --revision FULL_COMMIT_SHA \
  --toolchain stable --target x86_64-unknown-linux-gnu
```

The working-tree operation selects current on-disk contents. Staged and unstaged
edits, tracked deletions and untracked files are captured. Git-ignored input files
are also captured because explicit Cargo include rules may package them. Cargo
owns the final include/exclude decision. Scratch Git tracking state mirrors the
current index so forced additions of ignored files and cached removals retain
Cargo's effective package membership; file contents still come from the working
tree, including unstaged edits. Build target directories and linked
`.worktrees` administration are excluded from source capture. Submodules are not
initialized by the operation; source symlinks are rejected before materialization.
The command never stages, commits, updates caller manifests/lockfiles or advances
the active baseline. Cargo inspection, version materialization and packaging run
in a disposable Git clone. The wrapper is automatically built in the tooling
checkout's ordinary target directory.

Local results record `source.mode`, base `source.revision` and `source.dirty`.
The dirty marker is conservative: ignored captured inputs may make it true even
when Git status is empty. A dirty result never identifies its contents solely by
HEAD. Concurrent source editing during capture is not a reproducible input.
Use explicit commit mode for reproducibility.

Commit mode accepts a full 40-character SHA and reads only that commit's source,
policy and active baseline. Ambient dirty edits and untracked files are excluded.
CI validation and #360 candidate production must select commit mode. Neither
mode discovers releases or consults crates.io publication history.

## Package producer

`.local/sbin/package-release-sources` is shared by package preflight and the
snapshot producer. It selects publishable local workspace members through
`release-crate-manifest` and uses the existing Cargo package operation with local
crates.io patches and `--no-verify`. The snapshot producer first invokes the
existing stable release version materializer exactly once in scratch. It does
not change version calculation or interpret project release tags. Its existing
stable-current-version requirement (#364) remains in force.

For #360, the internal producer is independently callable with Python 3.12:

```sh
uv run --no-project --no-config --python 3.12 \
  .local/sbin/prepare-semver-packages --repository "$PWD" \
  --revision FULL_COMMIT_SHA --toolchain stable \
  --target x86_64-unknown-linux-gnu --output /absolute/new-output
```

The output must be new and outside the selected repository. It contains
`coverage.json` and all normalized source packages under `packages/`. Descriptor
paths are relative to that directory. Schema 1 gains optional `source` provenance;
legacy baseline descriptors remain readable. Full current package mappings and
feature selections are retained even for never-published crates. Verdict
filtering never reduces the candidate payload to checked crates. This is input
to #360, not candidate archive generation or stable adoption.

## Verdict and diagnostics

Currently publishable packages present in the explicitly selected active baseline
have released obligations. Private/no-longer-publishable and never-published
packages are excluded from SemVer API/intent verdicts. Keep old/new package inputs
available for the dependency closures of checked crates. Do not discard the
active baseline or bootstrap a missing/corrupt one. New package coverage becomes
released only through stable adoption owned by #360.

For each checked crate, `workspace.metadata.next-release` must declare `patch`,
`minor` or `major`. The maximum of that crate's comparison and feature-lifecycle
evidence is compared with its declared upper bound. The workspace-wide maximum
is never applied to other crates. Binary-only selections are explicit exclusions
from Rust API analysis, not CLI compatibility proofs. Removing released features
or an API target of a checked crate remains Major evidence. Build/tool failures
remain analysis errors even under Major intent.

The materializer independently requires valid release metadata for every package
it will release, including new packages. That is a release-version prerequisite,
not a SemVer obligation for an unpublished crate.

Standard output is one JSON report: `passed`, `exceeds-intent`, or
`analysis-error`. The latter contains diagnostics without an impact verdict.
Exit 0 means all checked crates fit their intent; other outcomes exit 1. Invalid
CLI syntax exits 2. `crates` contains declared/required levels, outcomes and
configuration/kind evidence. The existing detailed comparison evidence,
exclusions and limitations are preserved. The temporary local report omits
`next_coverage` because those package paths expire; the explicit snapshot producer
retains the complete descriptor and payload for #360.

The existing corrected analyzer, uv/Python 3.12, Cargo/rustup, Bash, Git, jq, Taplo
and cargo-edit are prerequisites. The command prepares xbuild automatically and
never installs/substitutes an analyzer. Packaging may resolve registry inputs;
comparison remains offline and requires dependencies in Cargo's cache on both
sides. Missing offline dependencies fail clearly rather than replacing history.

## Execution cost

`check-test-flow --step=semver` is opt-in. It uses the active installed Rust
toolchain and its host target, and the repository's checked-in product mappings;
`--all-features` does not replace that policy. A SemVer-only invocation skips
the normal flow's payload-copy preparation so local edits are analyzed unchanged.
Defaults continue to skip it.
JSON `timings` reports package production, analysis and total elapsed seconds,
including source/tool preparation in the total. Record tool versions, source,
target, cache conditions and runner resources with developer/CI measurements.
Default enablement requires a separate maintainer decision. Required CI wiring,
baseline adoption and performance cache optimization remain separate work.
