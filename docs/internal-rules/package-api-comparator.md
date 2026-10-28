# One-configuration API comparison

The internal `.local/sbin/compare-package-api` command implements #357 for the
coverage coordinator (#358) and release-intent consumer (#359). It compares a
saved released API resource with a current API, using the
[release policy](semver-release-intent.md). It does not select publication
history, release intent or baseline adoption.

## Inputs

The command requires absolute `--request`, `--diff-wrapper` and
`--semver-checks` paths. The request contains `old`, `new`, `target`, `toolchain`
and optionally an absolute `cargo_context` directory.

The old side is a generated resource:

```json
{
  "rustdoc": "/baseline/rustdoc/example/0.json",
  "coverage": {
    "name": "example",
    "version": "1.0.0",
    "features": [],
    "default_features": true,
    "resolved_features": [],
    "feature_inventory": [],
    "target_kind": ["lib"],
    "schema": 61,
    "documented_target": "x86_64-unknown-linux-gnu"
  },
  "producer": {
    "toolchain": "stable",
    "target": "x86_64-unknown-linux-gnu",
    "host": "x86_64-unknown-linux-gnu",
    "rustc": "recorded producer identity",
    "cargo": "recorded producer identity",
    "rustdoc": "recorded producer identity"
  }
}
```

Historical source-package requests are rejected. The comparator never runs
Cargo metadata, dependency resolution, compilation or rustdoc for the old side.
Missing, corrupt or unsupported resources fail analysis; there is no rebuild
fallback or compiler substitution. Producer information describes how the saved
resource was generated; consumers do not recreate that environment.

The new side is either another generated resource, or an explicit current
source-package selection:

```json
{
  "package": "/prepared/packages/example-1.0.1",
  "dependencies": [],
  "features": [],
  "default_features": true
}
```

Source inputs are extracted Cargo packages with normalized manifests and their
complete local packaged dependency closure. The comparator adds local crates.io
patches for those dependencies and validates their selected versions. For new
coverage with no released predecessor, `old: null` validates the generated
current JSON against itself. It generates that JSON once, not twice.

## Current API production

Only the current source selection is built. Temporary source copies allow Cargo
to update package lockfiles without changing the caller's files. Builds inherit
the selected Cargo context's configuration, flags, compiler wrappers (including
sccache) and normal persistent target directory. The context is the explicitly
selected repository for repository validation, or the invocation directory for
the standalone command when `cargo_context` is omitted. Cargo may fetch normal
registry dependencies. No isolated compiler environment, forced profile or fresh
per-comparison build directory is imposed. CI owns its controlled environment.

The explicit toolchain and target apply to current production. Compiler
executable overrides remain effective. The operation scopes `RUSTC_BOOTSTRAP=1`
to generate rustdoc JSON and records current compiler/Cargo/rustdoc identities.
Proc-macro JSON is produced for the host; libraries use the requested target.
Each generated JSON is saved before a subsequent build can overwrite its shared
target-directory output. Identity, version, target, public coverage and schema
are validated before analysis.

A resource-to-resource comparison invokes no compiler or Cargo command. This
allows #360 to validate already generated release candidates before adoption.

## Analyzers and verdicts

Both analyzers consume the saved old JSON and current JSON directly:

- `public-api` 0.52.0 through `xbuild public-api-diff` retains auto, blanket and
  derived impls and omits value-parameter names;
- corrected `cargo-semver-checks 0.51.0-sqlfun.1` compares JSON directly with an
  explicit Patch release type. Stock 0.51.0 is rejected because of #826.

A second text copy of the public API is unnecessary: both consumers already
accept the saved JSON. The baseline format does not require identical old/new
compiler versions. Both analyzers must successfully read and analyze each
artifact; unsupported schemas produce an analysis error, not a baseline rebuild.

The adapter validates the SemVer tool's run boundaries, 229 completed checks,
31 skipped checks, denied/warning findings and exit status. Major/Minor warnings
contribute to required impact. Unknown/incomplete output fails analysis.
Manifest lint overrides are not assumed to apply to JSON input.

No structural difference starts at Patch; added-only starts at Minor;
removed/changed API starts at Major. Take the maximum with SemVer findings and
removed released manifest features. Full feature inventories and resolved
features are saved with resources, independently of the JSON API item graph.
Build/tool errors remain `analysis-error` without an impact verdict.

Success exits 0 with `status: "detected"`, impact, structural diff, SemVer
counts/diagnostics, old/new coverage, current and baseline producer information,
and limitations. Exit 0 does not mean the change fits its release intent.
Analysis failures exit 1; invalid CLI syntax exits 2.

Structural analysis does not prove runtime behavior or accepted macro inputs.
The maintained lightweight contracts run through `xbuild/tests/package_api.rs`
and the standard workspace/nextest archive groups. They include the no-old-build
boundary and corrupt/legacy resource rejection without recurring compiler
replays. Historical compiler-test deletions remain documented in
[the workspace test cost review](workspace-test-cost.md).
