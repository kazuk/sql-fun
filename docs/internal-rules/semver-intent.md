# SemVer release intent validation

This guide is for contributors checking compatibility and maintainers selecting
release intent and adopting stable-release baselines. The check measures the
public Rust API of materialized source packages against the active baseline.
`workspace.metadata.next-release` is the maximum accepted detected impact for
each publishable workspace crate: `patch < minor < major`. An exceeded intent
fails the check; maintainers may explicitly raise intent or revise the API.

## Routine local check

The local step is optional early feedback. Without `cargo-semver-checks`, the
standard flow reports that it is skipping SemVer and continues. With the tool
installed, it runs the check; Python 3.12+ and the existing contributor tools
(`cargo-edit`, `taplo`, Cargo, Git, Bash, and native Rust build dependencies) are
then required. Local tools are not rejected solely for differing versions.
Actual analysis failures and intent violations still fail an enabled local check.
CI pins cargo-semver-checks 0.51.0 and uses `--require-semver` to require installation
and the exact tested version; local optionality cannot bypass that gate. CI's
exit-code contract distinguishes API violations (100) from analysis failures (101).

```sh
.local/bin/check-test-flow --step=semver
```

The repository step explicitly selects this repository's current source,
`.local/semver_baseline.tar.gz`, and `x86_64-unknown-linux-gnu` with default crate
features. Tracked and nonignored local edits are included. The checkout's
manifests are not changed. Packages and rustdoc/build results are cached under
`target/semver-intent`; remove that directory to discard the cache. The first
check needs dependency downloads and API compilation. Later checks reuse the
cache, including rustdoc JSON keyed by compiler, environment, Cargo configuration,
and dependency lock state, without generating an archive, executing exhaustive
fixtures, querying release provenance, or adopting release state. Cargo
configuration inputs follow its normal package-workspace directory hierarchy and
`CARGO_HOME` (or the default Cargo home). Configurations with `include` disable
JSON reuse so changes to transitive configuration inputs are not missed.

Local API cache inputs come from Cargo's package file selection plus workspace
release/build configuration, lockfile, toolchain, and materialization helpers.
Repository infrastructure (such as workflows, internal tooling, CI setup, and
manual/report directories) and Markdown content are omitted from the local API
fingerprint for packages without build scripts; documentation does not affect
the measured API. Rust source files and Cargo target entrypoints are always
tracked, including sources located in support/documentation directories. Build-script
packages conservatively track Markdown too because scripts can generate APIs
from data files. Other packaged assets remain inputs, including embedded SQL,
templates, and text. Excluded files and files outside packages do not trigger API
rebuilds. Normal-CI candidate generation uses a separate complete-source cache
and freshly packages documentation edits; this optimization never makes candidate
archives omit the current documentation. Compilation outputs are isolated per
packaged snapshot because normalized package timestamps must not allow stale
build-script outputs or local dependencies to cross source revisions.

The command reads the selected repository files required to construct its
packages. Version materialization and package construction occur in a temporary
source copy; the API comparison reads the extracted package contents. Workspace
package dependencies are patched to those extracted packages, rather than to
unpublished registry versions or the original source tree. This compilation is
separate from #304's package-construction preflight.

The report names each crate, declared intent, detected required level, and
whether it exceeds intent. Exit code 0 means the detected impact is permitted,
1 means an exceeded intent, and 2 means invalid input or analysis failure.
Binary-only crates are reported as outside Rust API analysis coverage. New
crates have no previous API to compare. Removing an existing publishable crate
requires an explicit baseline transition rather than silently skipping it.

## Coverage and limitations

The supported CI scope is Linux amd64 with default features. Feature-specific
and other platform APIs are not covered by that check. For a separate explicit
scope, both candidate generation and comparison require `--target` and
`--default-features` and accept repeated
`--features` inputs; the additional features apply to every analyzed crate and
must exist there. Baseline scope must match the requested scope.

The reported requirement is the minimum detected by cargo-semver-checks lints,
with patch as the floor when no lint requires more. The tool does not detect all
SemVer changes: for example, ordinary additive functions are not generally
classified as minor, and behavioral compatibility, CLI behavior, MSRV changes,
and default-disabled APIs require review or other checks. The fixture suite
makes this limitation explicit. A passing result is not a complete SemVer proof.

The check probes patch and, if needed, minor thresholds using the tool's exit
statuses and cached API data. It passes the release level explicitly, including
for 0.x versions, rather than inferring allowed API changes from version numbers.
The check generates rustdoc JSON with Cargo in the extracted package workspace
and supplies it directly to the tool. This preserves the local dependency patch
map and includes proc-macro targets, which source selection in 0.51.0 excludes.
Rustdoc JSON is generated for both sides using the current toolchain; persisted
baselines contain packaged source rather than unstable rustdoc JSON.

## Candidate generation and initial baseline

Normal CI creates `semver_baseline.tar.gz` for every relevant revision without
knowing its eventual release classification. It uploads the candidate in the
internal `semver-baseline` Actions artifact, even if comparison subsequently
finds an intent violation. Candidates are not GitHub Release assets.

For initial setup, a maintainer must explicitly select the approved source
revision. Check out that revision in a separate worktree, then run the generator
from the tooling checkout with the selected source and an output outside it:

```sh
/path/to/tooling/.local/bin/semver-intent candidate \
  --source /path/to/approved-source --target x86_64-unknown-linux-gnu \
  --default-features \
  --cache /path/to/cache --output /path/to/semver_baseline.tar.gz
```

Place that approved archive at `.local/semver_baseline.tar.gz` and commit it.
Missing baselines fail with setup guidance. Neither the current revision nor a
registry release is automatically selected as the baseline.

The archive contains a schema descriptor and the complete extracted publishable
package tree, including the package workspace/dependency patch map. The descriptor
records crate versions, source revision/content digest, target, and extra features.
These fields describe the candidate; they are not an independent provenance
proof. Archive extraction uses Python's data filter and checks the descriptor
and required package manifests before analysis. The package/cache construction
is serialized to avoid interfering simultaneous invocations.

## Stable release adoption contract (#259)

The active path is exactly `.local/semver_baseline.tar.gz`. The release transaction
must select the internal candidate from a successful required normal-CI run for
the exact release target commit, using GitHub Actions metadata as the provenance
trust boundary. It must not perform registry/latest-version discovery or create
a pointer file. The producer transaction itself remains owned by #259.

After stable release completion, copy that selected candidate into the checkout
used for the release-state back-commit:

```sh
.local/bin/adopt-semver-baseline --candidate /path/to/semver_baseline.tar.gz \
  --release-tag v1.0.0 \
  --destination /path/to/back-commit/.local/semver_baseline.tar.gz
```

Include the changed file in the same release-state back-commit as the materialized
versions. The helper replaces the file atomically for a valid stable product tag.
Snapshot tags leave it unchanged. The helper does not select CI runs, generate
candidates, commit, publish, or independently re-prove artifact provenance.

## Next-major compatibility debt

When intentionally keeping a compatibility workaround, add a temporary Markdown
specification in the affected crate's `next-major/` directory, outside `src`.
Use a descriptive filename such as `next-major/remove-legacy-query-adapter.md`.
Start with the [migration specification template](next-major-template.md).
Each document must identify why compatibility is retained, the intended API and
removal/simplification, affected APIs/tests/issues, and completion criteria.
Delete the specification in the same PR that completes the migration. This is
an implementation backlog; Git history retains completed specifications.

## Validation and CI rollout

```sh
python3 .local/tests/test_semver_intent.py
.local/bin/check-test-flow --step=version-materialization
```

Fixture tests cover actual patch/minor/major lint thresholds, proc macros,
analysis failures, metadata validation, baseline scope/extraction, and release
adoption. They are separate from the routine local SemVer path. New CI jobs
must pass on the final PR head before the implementation is declared complete.
The aggregate `build` job requires the SemVer job and explicitly rejects any
failed, cancelled, or skipped dependency, so a failed gate cannot become a
successful aggregate check. The adopted CI image provides Python and the pinned
analysis tool; the job checks the required tool version rather than installing
another copy.

The analysis contract follows [cargo-semver-checks 0.51.0](https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0).
