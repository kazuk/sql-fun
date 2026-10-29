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
checkout's ordinary target directory, explicitly for the compiler host so it
can run locally even when an ambient Cargo target selects cross-compilation.
Its executable path comes from Cargo's build artifact output.

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

The same validation invocation can save the current-source APIs as a candidate:

```sh
.local/bin/check-semver --repository "$PWD" --revision FULL_COMMIT_SHA \
  --toolchain stable --target x86_64-unknown-linux-gnu \
  --candidate-output /absolute/new/semver_baseline.tar.gz
```

The output must be new and outside the selected repository; its parent must
exist. Candidate output requires explicit commit mode. Successful validation
archives the schema-2 descriptor and current rustdoc JSON, reusing outputs from
that execution. Only missing current configured selections, including
never-released crates, require additional generation. Binary-only packages keep
inventory/mappings without API JSON. Source packaging and version materialization
run once through the existing path. Failed analysis, denied intent or missing
candidate coverage emits no final archive. Existing outputs are never replaced.

The archive contains `baseline.json` and `rustdoc/`, not source packages or
analyzer reports. Tar member metadata and gzip timestamps are normalized.
Candidate generation does not advance released coverage. There is no separate
candidate producer or initial-baseline creation mode; the approved #358 baseline
must already exist. Missing/corrupt state fails without bootstrapping.

### Explicit adoption

The internal release adapter uses Python 3.12 and explicit paths/classification:

```sh
uv run --no-project --no-config --python 3.12 \
  .local/sbin/adopt-semver-baseline \
  --candidate /absolute/selected/semver_baseline.tar.gz \
  --active "$PWD/.local/semver_baseline.tar.gz" --release-kind stable
```

Stable adoption validates the existing active archive and selected committed
candidate, requires matching targets, and atomically replaces the active archive
with the selected bytes. Repeating adoption yields the same state. Validation
failure leaves the active file intact. A stable Major release may remove old
coverage; inventories need not grow monotonically. Snapshot/prerelease kinds
are explicit no-ops, preserving active bytes and modification time without
opening the candidate. Unknown or omitted classification fails.

The archive reader rejects unsupported schemas, malformed resources, inconsistent
identities/mappings, corrupt digests, missing JSON, duplicate archive members,
absolute/escaping paths, links and special files. Adoption builds no APIs and
runs no analyzers. It has no initialization operation.

Issue #361 owns candidate upload and CI gating. #329 selects/downloads the candidate
through the existing exact-SHA CI-run contract and calls this operation. Actions
metadata remains the provenance trust boundary. #259/#330 own transaction
completion and the explicit active-baseline back-commit during stable release
preparation; this local operation neither publishes nor queries releases.
The candidate is internal CI data and never a GitHub Release asset.

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
`next_coverage` because those package paths expire; the candidate-output option
retains complete generated resources and metadata in the archive.

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

The standard `version-materialization` check runs the release-input integration
contracts through `test-release-publication`: real baseline extraction/replay
and Git/Cargo source membership. They retain all assertions without running
archive I/O and subprocess setup under nextest's unit-test time budget. The
lightweight comparator and intent contracts remain in xbuild's nextest suite.

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
release-side adoption wiring and performance cache optimization remain separate
work.
