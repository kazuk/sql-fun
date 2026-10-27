# One-configuration package API comparison

This internal interface implements #357 for the coverage coordinator (#358)
and release-intent consumer (#359). It follows the
[approved policy](semver-release-intent.md) and
[measured analyzer limits](semver-analyzer-probe.md). It does not select
publication history, feature sets, release intent, or baseline archives.

## Boundary and invocation

`.local/sbin/compare-package-api` is a script, outside the installed product CLI.
It owns preparation, execution, classification, cleanup and integration. Rust is
used only to obtain structured public API differences unavailable from the
upstream CLI. `xbuild public-api-diff --old OLD_JSON --new NEW_JSON` is that thin
wrapper; it is not a package/release orchestration library.

The script takes three required arguments:

```sh
.local/sbin/compare-package-api \
  --request /absolute/path/comparison.json \
  --diff-wrapper /absolute/path/xbuild \
  --semver-checks /absolute/path/cargo-semver-checks
```

The request is explicit, including empty lists and both default-feature policies:

```json
{
  "old": {
    "package": "/absolute/path/old-package",
    "dependencies": ["/absolute/path/old-local-dependency"],
    "features": [],
    "default_features": false
  },
  "new": {
    "package": "/absolute/path/new-package",
    "dependencies": ["/absolute/path/new-local-dependency"],
    "features": ["new-feature"],
    "default_features": true
  },
  "target": "x86_64-unknown-linux-gnu",
  "toolchain": "stable"
}
```

Package directories contain already extracted Cargo source-package contents,
including normalized, self-contained `Cargo.toml` files. The example's
`stable` names an explicitly supplied installed toolchain. The comparator uses
that toolchain’s Cargo, rustc and rustdoc together, even if ambient executable
paths or Cargo-home settings name a different compiler.

Supply the complete local packaged dependency closure on each side, including
transitive and proc-macro dependencies. No workspace discovery or source/version
materialization occurs. Source path dependencies, external workspace references,
workspace inheritance, target sources outside the package, and symlinks are
rejected. Each supplied dependency name must be unique and resolve
to the supplied package/version through operation-local crates.io patches.
Other registry dependencies must already be available to Cargo offline. The
comparator does not fetch missing dependencies or substitute adjacent sources.
A missing requested feature fails analysis; it is never silently omitted.

The script copies only the selected package inputs into a fresh temporary
operation directory. Cargo metadata, lockfile updates, builds and documentation
run there with separate old/new target directories. Inputs remain unchanged,
and scratch data is deleted on success or failure. It passes each side's own
feature/default-feature selection to both Cargo metadata and JSON production.
Caller compiler flags/wrappers and repository Cargo configuration are excluded;
Cargo's home remains available for offline registry/cache access. Ordinary Cargo
home configuration still applies, subject to explicit compiler/flag overrides
and validation of local dependency resolution.

## Producer and analyzer contract

Analyzer versions are pinned independently of compiler versions:

- `public-api` library 0.52.0, with rustdoc-types 0.57.3 in `Cargo.lock`;
- `cargo-semver-checks` executable **0.51.0-sqlfun.1**, built from upstream
  0.51.0 with the [#826 trait-presence query correction](semver-shadowed-trait.md).
  Stock 0.51.0 is rejected; its identical-input false positive is reproducible.

The #356 producer (Rust/Cargo 1.99.1, JSON schema 61) records a successful
probe, not a mandatory contributor toolchain. Use the explicitly requested
installed toolchain, with scoped `RUSTC_BOOTSTRAP=1` for JSON production. Record
its actual rustc verbose identity, Cargo/rustdoc versions, host and requested
target in the report. Each side records the actual generated schema version.
There is no compiler-version equality gate or automatic compiler substitution.

Both pinned analyzers must successfully consume the same freshly generated JSON
files. Validate package/version identity, private-item coverage, target selection
and complete successful production before comparison. A positive schema marker
alone does not establish compatibility: the SemVer reader checks supported
formats, and both analyzers must parse and complete their respective analyses.
Unsupported input returns an analysis error with the actual schema, producer and
underlying diagnostics. Update and re-probe the pinned analyzers when necessary;
do not install an older compiler to hide incompatibility.

The explicit `--target` controls library builds; Cargo documents proc-macro
targets for the host, placing their JSON in the host output directory. The
script selects that path from the target kind, without replacing a macro target
with a library target. Unavailable target support fails generation. Arbitrary
archived JSON is not an input contract.

The Rust wrapper retains auto, blanket and derived impls and omits value
parameter names, matching the probe. It emits the library's added/removed lists
and changed old/new pairs as JSON. Its successful empty output categories are
not a behavioral compatibility proof. The library's debug-oriented
`missing_item_ids` is not a completeness validator: valid ordinary types refer
to external/private items not included in rustdoc's index.

The SemVer adapter invokes JSON comparison once with explicit Patch release
type. The pinned CLI has no structured finding-level output, and its public
Rust report API does not expose warning bump counts. The script therefore
validates its textual run boundaries, completion counts (229 selected checks,
31 skipped allow-by-default checks), finding headers, required-level summaries
and status. Both denied and warning-level Major/Minor counts contribute.
Unknown/incomplete output, unsupported status or inconsistent counts fail
analysis. Exit 100 with valid completed findings is a successful analysis;
exit 0 with Major warnings still requires Major. Manifest lint overrides are
not assumed to apply to JSON input.

## Result and limits

Standard output contains one JSON object. Exit 0 means completed analysis with
`status: "detected"`, an `impact` of `patch`, `minor` or `major`, structured
`diff`, `removed_features`, SemVer counts/diagnostics, old/new coverage,
producer details and visible `limits`. It does not mean the change is within
release intent. Exit 1 returns `status: "analysis-error"` with diagnostics and
limits, without an impact. Argument syntax errors use argparse diagnostics and
exit 2 before analysis starts.

No structural difference starts at Patch; added-only starts at Minor;
removed/changed starts at Major. Take the maximum with all applicable SemVer
findings, including warnings, and with Major for removed old manifest features.
Full feature inventories come from Cargo metadata because JSON lacks them.
Adding feature names alone does not imply a removal. Requested coverage that
cannot be generated is an error, distinct from an observed inventory removal.
An unrecognized analyzer representation fails validation rather than pretending
that parsing failure is a successfully observed unclassifiable API change.

Publication history and comparisons needed to preserve every released surface
belong to #358. This operation evaluates exactly the supplied pair; candidate
additions never cancel removals visible in that pair. Macro expansion, accepted
macro input and runtime behavior remain outside structural proof and need
complementary tests/review. A known unresolved behavioral concern cannot be
cleared by an empty structural diff.

## Maintained checks and preparation

The existing `workspace` check runs `xbuild`'s integration test through Cargo or
nextest, including its CI archive group. `tests/package-api/test_compare.py`
retains inexpensive explicit-input, preparation, wrapper-error, JSON and report
checks. The #828 cost review also discarded the repeated Cargo metadata,
producer, source-boundary and analyzer-version rejection cases after CI timed
out the reduced aggregate near one second. The comparator runtime
requires Python 3.11+ (`tomllib`), Bash, rustup and Cargo. Maintained tests use
`uv run --no-project --no-config --python 3.12` through existing contributor/CI
setup. Runtime nextest paths support archive relocation.

The #828 [test cost review](workspace-test-cost.md) discarded the repeated real
Cargo/rustdoc/analyzer comparisons and complete historical-baseline replay.
These assertions are not retained as ignored or manual tests. Compiler/analyzer
integration, feature/proc-macro API comparisons, producer/schema mutation, and
complete-baseline Patch verdicts are no longer recurring test coverage. The
comparison implementation and its runtime analysis guarantees are unchanged.

`.local/sbin/prepare-package-api-tests` records the active contributor toolchain,
including an explicit rustup override, and verifies the corrected analyzer
provided by the documented development/CI builder image. It never installs or
patches tools. A missing or stock executable fails with the required identity.
`ci/github-actions/Dockerfile` owns the checksum-pinned source download, query
correction, locked build and semantic smoke tests. Image publication and adoption
follow `ci/README.md`; the comparator must not be adopted before that image.

The [feature-coverage coordinator](semver-feature-coverage.md) consumes this
interface under the repository's additive-feature rule. Its maintained tests
share the existing `workspace` integration path. Production comparison remains
offline; the discarded historical replay no longer provisions dependencies.
