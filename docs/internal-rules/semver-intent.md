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
(`cargo-edit`, `taplo`, Cargo, Git, Bash, and native Rust build dependencies)
are
then required. Local tools are not rejected solely for differing versions.
Actual analysis failures and intent violations still fail an enabled local
check.
CI pins cargo-semver-checks 0.51.0 and uses `--require-semver` to require
installation
and the exact tested version; local optionality cannot bypass that gate. CI's
exit-code contract distinguishes API violations (100) from analysis failures
(101).

```sh
.local/bin/check-test-flow --step=semver
```

The repository step explicitly selects this repository's current source,
`.local/semver_baseline.tar.gz`, and `x86_64-unknown-linux-gnu` with the named
configurations in
`.local/semver-feature-sets.toml` and default crate features enabled. Tracked
and nonignored local edits are included. The checkout's
manifests are not changed. Packages and rustdoc/build results are cached under
`target/semver-intent`; remove that directory to discard the cache. The first
check needs dependency downloads and API compilation. Later checks reuse the
cache, including rustdoc JSON keyed by compiler, environment, Cargo
configuration,
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
tracked, including sources located in support/documentation directories. Linked
source inputs track referenced file contents as well as the link path.
Build-script
packages conservatively track Markdown too because scripts can generate APIs
from data files. Other packaged assets remain inputs, including embedded SQL,
templates, and text. Excluded files and files outside packages do not trigger
API
rebuilds. Normal-CI candidate generation uses a separate complete-source cache
and freshly packages documentation edits; this optimization never makes
candidate
archives omit the current documentation. Compilation outputs are isolated per
packaged snapshot because normalized package timestamps must not allow stale
build-script outputs or local dependencies to cross source revisions.

The command reads the selected repository files required to construct its
packages. Selected symlinks must reference readable files; directory links and
broken links are rejected. File links are captured as ordinary file contents in
the isolated checkout, so version materialization cannot write through them into
the contributor checkout. Version materialization and package construction occur
in a temporary
source copy; the API comparison reads the extracted package contents. Workspace
package dependencies are patched to those extracted packages, rather than to
unpublished registry versions or the original source tree. This compilation is
separate from #304's package-construction preflight.

The report names each feature set and crate, declared intent, detected required
level, and
whether it exceeds intent. Exit code 0 means the detected impact is permitted,
1 means an exceeded intent, and 2 means invalid input or analysis failure.
Binary-only crates are reported as outside Rust API analysis coverage. New
crates have no previous API to compare. Removing an existing publishable crate
requires an explicit baseline transition rather than silently skipping it.

## Coverage and limitations

The supported CI scope is Linux amd64 across the named `postgres` and `sqlite`
product configurations in `.local/semver-feature-sets.toml`. Both enable default
Cargo features. `postgres` adds no features; `sqlite` enables each publishable
crate's `sqlite` feature where it exists, including client/server APIs.
Binary-only
crates still require explicit mappings even though their CLI behavior is outside
Rust API analysis. Other platforms and feature combinations are not covered.

Each `[feature-set.<name>]` table maps **every publishable crate** to a list of
additional Cargo feature names. An empty list explicitly chooses only default
features for that crate. Missing mappings, private/unknown crates, unknown
feature
names, malformed lists, and an empty policy are errors. There is no global
feature
list, implicit mapping, or `--all-features` substitution. Maintainers must
update
this policy when changing the publishable crate set or supported configurations.

Candidate generation and comparison require `--target`, `--default-features`,
and an explicitly selected `--feature-sets` file. The baseline records
configuration identities and per-crate mappings. Configurations present on both
sides are compared normally; an exceeded intent in any such configuration fails
the gate. A configuration present only in the current policy has no previously
released API and is reported as new without a SemVer violation. Its eventual
stable-release candidate establishes its first baseline; subsequent checks then
compare it normally. Removing a baseline configuration or changing an existing
configuration's per-crate mapping requires an explicit baseline transition.
New crates need complete current mappings and have no previous API. Invalid
baseline mappings/features are rejected using the packaged manifests.

The reported requirement is the minimum detected by cargo-semver-checks lints,
with patch as the floor when no lint requires more. The tool does not detect all
SemVer changes: for example, ordinary additive functions are not generally
classified as minor, and behavioral compatibility, CLI behavior, MSRV changes,
and feature combinations outside the configured sets require review or other
checks. The fixture suite
makes this limitation explicit. A passing result is not a complete SemVer proof.

The check probes patch and, if needed, minor thresholds using the tool's exit
statuses and cached API data. It passes the release level explicitly, including
for 0.x versions, rather than inferring allowed API changes from version
numbers.
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
  --feature-sets /path/to/tooling/.local/semver-feature-sets.toml \
  --cache /path/to/cache --output /path/to/semver_baseline.tar.gz
```

Place that approved archive at `.local/semver_baseline.tar.gz` and commit it.
Missing baselines fail with setup guidance. Neither the current revision nor a
registry release is automatically selected as the baseline.

The archive contains a schema descriptor and the complete extracted publishable
package tree, including the package workspace/dependency patch map. The
descriptor
records crate versions, source revision/content digest, target, default-feature
selection, and named per-crate feature mappings (schema 4).
These fields describe the candidate; they are not an independent provenance
proof. Archive extraction uses Python's data filter and checks the descriptor
and required package manifests before analysis. The package/cache construction
is serialized to avoid interfering simultaneous invocations.

## Stable release adoption contract (#259)

The active path is exactly `.local/semver_baseline.tar.gz`. The release
transaction
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

Include the changed file in the same release-state back-commit as the
materialized
versions. The helper replaces the file atomically for a valid stable product
tag.
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
adoption. They also exercise SQLite-only API removal and strict feature-policy
validation, including new configurations becoming checked after stable adoption,
removed configurations, and changed existing mappings. They are separate from
the routine local SemVer path. New CI jobs must pass on the final PR head before
the implementation is declared complete. The aggregate `build` job requires
SemVer and explicitly rejects any failed, cancelled, or skipped dependency, so
a failed gate cannot become a successful aggregate check. The adopted CI image
provides Python and the pinned analysis tool; the job checks the required tool
version rather than installing another copy.

The analysis contract follows [cargo-semver-checks
0.51.0](https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0).
