# Repository release-intent integration

`check-semver` implements #359 for maintainers validating local changes and for
future CI adapters. It reuses the #357 comparator and #358 feature lifecycle;
see [policy](semver-release-intent.md), [coverage](semver-feature-coverage.md),
and [package comparison](package-api-comparator.md).

## Source selection and package preparation

The standalone command requires an absolute repository root, one source mode,
an installed toolchain and an explicit target:

```sh
.local/bin/check-semver --repository "$PWD" --working-tree \
  --toolchain stable --target x86_64-unknown-linux-gnu
.local/bin/check-semver --repository "$PWD" --revision FULL_COMMIT_SHA \
  --toolchain stable --target x86_64-unknown-linux-gnu
```

The working-tree operation selects current on-disk contents. Staged and unstaged
edits, tracked deletions and untracked files are captured. Git-ignored input
files
are also captured because explicit Cargo include rules may package them. Cargo
owns the final include/exclude decision. Scratch Git tracking state mirrors the
current index so forced additions of ignored files and cached removals retain
Cargo's effective package membership; file contents still come from the working
tree, including unstaged edits. Build target directories and linked
`.worktrees` administration are excluded from source capture. Submodules are not
initialized by the operation; source symlinks are rejected before
materialization.
The command never stages, commits, updates caller manifests/lockfiles or
advances
the active baseline. Cargo inspection, version materialization and packaging run
in a disposable Git clone. The wrapper is automatically built in the tooling
checkout's ordinary target directory.

Local results record `source.mode`, base `source.revision` and `source.dirty`.
The dirty marker is conservative: ignored captured inputs may make it true even
when Git status is empty. A dirty result never identifies its contents solely by
HEAD. Concurrent source editing during capture is not a reproducible input.
Use explicit commit mode for reproducibility.

Commit mode accepts a full 40-character SHA and reads only that commit's source,
policy and active baseline. Ambient dirty edits and untracked files are
excluded.
CI validation and #360 candidate production must select commit mode. Neither
mode discovers releases or consults crates.io publication history.

## Current source and resource producers

`.local/sbin/package-release-sources` is shared by package preflight and the
snapshot producer. It selects publishable local workspace members through
`release-crate-manifest` and uses the existing Cargo package operation with
local
crates.io patches and `--no-verify`. The snapshot producer first invokes the
existing stable release version materializer exactly once in scratch. It does
not change version calculation or interpret project release tags. Its existing
stable-current-version requirement (#364) remains in force.

For #360, the internal producer is independently callable with Python 3.12:

```sh
uv run --no-project --no-config --python 3.12 \
  .local/sbin/prepare-semver-baseline --repository "$PWD" \
  --revision FULL_COMMIT_SHA --toolchain stable \
  --target x86_64-unknown-linux-gnu --output /absolute/new-output
```

The output must be new and outside the selected repository. It contains a
schema-2 `baseline.json` and generated rustdoc JSON, not source packages. It
retains the complete current publishable inventory and product selections,
including never-published crates. Current version materialization and packaging
remain internal preparation steps. The explicit producer does not require an
existing baseline or read released
intent obligations. Resource generation occurs once for a candidate; #360 owns
validation, archive creation and stable adoption.

The active baseline uses the same generated-resource format. Validation reads
its saved APIs and coverage metadata without Cargo metadata, dependency builds
or rustdoc generation on the old side. The initial archive has been migrated at
the same approved revision; no baseline advancement occurs. Legacy source-only
baselines, missing resources and unsupported schemas fail explicitly, without
rebuilding history or selecting another compiler.

## Verdict and diagnostics

Currently publishable packages present in the explicitly selected active
baseline
have released obligations. Private/no-longer-publishable and never-published
packages are excluded from SemVer API/intent verdicts. Only current builds
require source/dependency inputs; historical obligations
come entirely from saved resources. Do not discard the
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
`next_coverage` because those package paths expire; the explicit snapshot
producer
retains complete generated resources and metadata for #360.

The existing corrected analyzer, uv/Python 3.12, Cargo/rustup, Bash, Git, jq,
Taplo
and cargo-edit are prerequisites. The command prepares xbuild automatically and
never installs/substitutes an analyzer. Packaging may resolve registry inputs;
comparison uses the selected repository's ordinary Cargo configuration, compiler
wrappers/flags and persistent target directory. Dependencies may be fetched as
in
a normal developer build. CI is responsible for provisioning and controlling its
own validation environment.

## Execution cost

`check-test-flow --step=semver` is opt-in. It uses the active installed Rust
toolchain and its host target, and the repository's checked-in product mappings;
`--all-features` does not replace that policy. A SemVer-only invocation skips
the normal flow's payload-copy preparation so local edits are analyzed
unchanged.
Defaults continue to skip it.
JSON `timings` reports package production, analysis and total elapsed seconds,
including source/tool preparation in the total. Compiler wrappers such as
sccache
are inherited, and builds reuse the normal Cargo target directory across crates,
current selections and invocations. The old side is never built. Cache hits
depend on Cargo/compiler-cache keys;
source packaging still uses temporary files to avoid changing release versions
in the developer checkout. Record tool versions, source,
target, cache conditions and runner resources with developer/CI measurements.
Default enablement requires a separate maintainer decision. Required CI wiring,
baseline adoption and performance cache optimization remain separate work.
