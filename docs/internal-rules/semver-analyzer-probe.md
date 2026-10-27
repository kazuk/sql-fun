# cargo-semver-checks capability probe (#356)

## Purpose and status

These measured results inform the release-intent policy in
[semver-release-intent.md](semver-release-intent.md) and the analyzer adapter in
later implementation issues. They are investigation evidence, not approval of a
supplementary classifier or a claim of complete SemVer coverage. The
reproducible
fixture definitions below are maintained with this report; repeat them when
changing the authoritative analyzer version or input path.

Measured on 2026-10-27 with **cargo-semver-checks 0.51.0**, rustc
`1.99.1 (58cf8f9c3 2026-10-13)`, Cargo `1.99.1 (5f94df478 2026-08-27)`,
and host/target `x86_64-unknown-linux-gnu`. This is the analyzer version already
pinned in `ci/github-actions/Dockerfile`. All fixtures are dependency-free and
independent of sql-fun baseline archives, packaging, caching and CI integration.
No contributor setup or required command changes are introduced.

## Reproduction contract

For each row, create separate `old/` and `new/` directories with `src/lib.rs`
containing the indicated Rust source and this `Cargo.toml`:

```toml
[package]
name = "semver-probe"
version = "1.0.0"
edition = "2021"

[workspace]

[lib]
proc-macro = false

[features]
```

Use `proc-macro = true` for macro rows. Append the indicated feature
declarations
to `[features]`. Both versions deliberately remain `1.0.0`: the explicit release
level controls the experiment without relying on version-number inference.
Run each row three times, replacing `patch` with `minor` and `major`:

```bash
cargo semver-checks check-release \
  --manifest-path new/Cargo.toml --baseline-root old \
  --only-explicit-features --release-type patch --color never
```

Append the feature flags specified in the table. `--only-explicit-features`
disables the analyzer's feature-selection heuristic; no default or ambient
feature selection is part of these experiments.

The table records actual process exit codes in patch/minor/major order:

- `0`: completed with no denied findings for the requested release level.
- `100`: completed with denied findings exceeding that level.
- `101`: analysis could not complete; this is not a compatibility verdict.

This distinction also follows the pinned version's
[`src/main.rs`](https://github.com/obi1kenobi/cargo-semver-checks/blob/v0.51.0/src/main.rs)
(`LINT_FAILURE_EXIT_CODE`, `ERROR_EXIT_CODE`, `check_exit_code`). Other
unexpected
statuses or missing results must also be treated as analysis failures, never a
pass. A major-level success can mean checks were skipped; it does not establish
that all surfaces were analyzed.

## Library and Cargo feature fixtures

`A` below means `pub fn api() {}`; `B` means
`pub fn api() -> u8 { 1 }`. An empty source contains no items. Sources shown in
a
cell are complete after expanding these abbreviations.

| Case | Old source | New source | Features and extra flags | Exit codes | Observation |
| --- | --- | --- | --- | --- | --- |
| Unchanged | `A` | `A` | None | 0/0/0 | No denied finding. |
| Body change | `B` | `pub fn api() -> u8 { 2 }` | None | 0/0/0 | Runtime behavior is not classified. |
| Public addition | `A` | `A pub fn added() {}` | None | 0/0/0 | Ordinary additive API does not require minor in this analyzer. |
| Public removal | `A` | Empty | None | 100/100/0 | `function_missing`. |
| Signature change | `A` | `pub fn api(_: u8) {}` | None | 100/100/0 | `function_parameter_count_changed`. |
| Non-exhaustive enum | `pub enum Api { A }` | `#[non_exhaustive] pub enum Api { A }` | None | 100/100/0 | `enum_marked_non_exhaustive`. |
| Deprecation | `A` | `#[deprecated] A` | None | 100/0/0 | `function_marked_deprecated`, a minor-level finding. |
| Must-use | `B` | `#[must_use] B` | None | 100/0/0 | `function_must_use_added`, a minor-level finding. |
| Selected API removal | `#[cfg(feature="extra")] A` | Empty | Both: `extra=[]`; `--features extra` | 100/100/0 | Enabled API removal is detected. |
| Unselected API removal | `#[cfg(feature="extra")] A` | Empty | Both: `extra=[]`; no extra flags | 0/0/0 | Disabled API is outside the analyzed surface. |
| Candidate-only feature | `A` | `A #[cfg(feature="extra")] pub fn added() {}` | Only new: `extra=[]`; `--current-features extra` | 0/0/0 | No nonexistent baseline feature is required; addition remains unclassified. |
| Feature removal, unchanged API | `A` | `A` | Only old: `extra=[]`; no extra flags | 100/100/0 | `feature_missing` uses manifest information even with the feature disabled. |
| Candidate-only shared flag | `A` | `A` | Only new: `extra=[]`; `--features extra` | 0/0/0 | Warns that baseline lacks `extra`, then succeeds; flags do not prove requested coverage exists on both sides. |
| Compilation failure | `A` | `compile_error!("probe failure");` | None | 101/101/101 | Build failure remains an error even for major intent. |

These observations establish both major- and minor-level detection, but not a
complete three-level classifier. In particular, trying patch, then minor, then
major and taking the first successful result misclassifies ordinary additions
as patch. A changed function body passing says nothing about behavioral
compatibility. Feature inventory and requested coverage must be validated
independently of successful exit codes.

## Procedural macro fixtures and input boundary

All macro sources start with `extern crate proc_macro;`. The following
definitions
supply complete source after that line:

```rust
// F: function-like macro
#[proc_macro]
pub fn api(input: proc_macro::TokenStream) -> proc_macro::TokenStream { input }

// D: derive macro (use alone, without F or T)
#[proc_macro_derive(Api)]
pub fn derive_api(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

// T: attribute macro (use alone, without F or D)
#[proc_macro_attribute]
pub fn api(_: proc_macro::TokenStream, input: proc_macro::TokenStream)
    -> proc_macro::TokenStream { input }
```

Each row returns **101/101/101** through the source-manifest command above:
`no crates with library targets selected, nothing to semver-check`. A proc-macro
crate is excluded by this source-selection path; tool availability does not
establish proc-macro coverage.

To test the separate rustdoc JSON input path, generate JSON for **both** sides:

```bash
RUSTC_BOOTSTRAP=1 cargo rustdoc --manifest-path old/Cargo.toml --lib -- \
  -Z unstable-options --output-format json
RUSTC_BOOTSTRAP=1 cargo rustdoc --manifest-path new/Cargo.toml --lib -- \
  -Z unstable-options --output-format json
cargo semver-checks check-release \
  --current-rustdoc new/target/doc/semver_probe.json \
  --baseline-rustdoc old/target/doc/semver_probe.json \
  --release-type patch --color never
```

Repeat the comparison for minor and major. This use of unstable rustdoc output
is an investigation method; it does not authorize a production
compiler/toolchain
strategy. Keep compiler and JSON format compatibility under explicit control in
any later adapter.

| Case | Old definition | New definition | JSON exit codes | Observation |
| --- | --- | --- | --- | --- |
| Function-like removal | F | Empty | 100/100/0 | Macro removal is detected. |
| Function-like addition | Empty | F | 0/0/0 | Addition is not classified as minor. |
| Expansion change | F | F with body `{ proc_macro::TokenStream::new() }` | 0/0/0 | Expansion behavior is not checked. |
| Derive removal | D | Empty | 100/100/0 | Derive removal is detected. |
| Attribute removal | T | Empty | 100/100/0 | Attribute macro removal is detected. |

JSON comparison demonstrates name-level removal coverage for these three macro
kinds, not compatibility of accepted input syntax, diagnostics, helper
attributes,
or generated Rust APIs. Existing macro/compiler tests remain necessary. Do not
silently skip proc-macro packages or replace their target with a different
library
target. A later implementation must explicitly choose and validate its JSON
production path before relying on these results.

## JSON metadata and parser failure controls

The JSON boundary has an additional limitation. Generate JSON for both sides of
**Feature removal, unchanged API** using the rustdoc commands above, then compare
those JSON files with all three release levels. The result is **0/0/0**, whereas
the source-manifest path returns **100/100/0** with `feature_missing`. Bare
rustdoc JSON does not carry the Cargo feature inventory needed by that check.
An adapter using JSON for library or macro APIs must also compare released Cargo
feature inventories; it cannot inherit the source-path result by assumption.

As a separate parser-failure control, replace the current JSON file with the
single byte `{`, retain the valid baseline JSON, and run the three comparisons.
The results are **101/101/101**. Malformed analyzer input is an analysis error,
independent of the requested release level.

Across the source matrix, macro JSON matrix and these two controls, this report
records **78 comparison invocations**. No external baseline or registry package
was used.

## Policy implications and remaining decisions

The consumers of these findings are #355's policy reviewers and #357–#361's
implementation work. Their observable requirements are:

1. Pin authoritative comparisons to **0.51.0** until a replacement is probed;
   record the compiler, target, selected features and source/JSON input
boundary.
2. Distinguish exceeded intent (`100`) from incomplete analysis (`101` or
   another
   unexpected failure). Neither permits a successful compatibility claim.
3. Report only a detected lower bound on required release level. For unsupported
   classifications, including ordinary public additions and macro expansion
   changes, retain **unresolved/inconclusive** status rather than a proven pass.
4. Keep released feature coverage visible and validate feature inventory and
   selection explicitly. Successful analysis of a disabled API proves nothing
   about that API; candidate-only features need asymmetric selections.
5. Preserve macro behavior tests and other complementary review for behavioral,
   platform, MSRV and untested API cases. This finite matrix does not establish
   completeness over Cargo's compatibility guide.

Item 3 carries forward acceptance example 8's existing inconclusive boundary.
Choosing a supplementary classifier, manual evidence/approval protocol, or a
narrower gate contract remains a **maintainer policy decision**. This report
does
not silently choose one. Until that decision is reviewed, the exact patch/minor/
major gate is not ready for implementation, and #355/#356 must not be described
as fully completed. No release metadata, baseline, cache or CI implementation is
part of this investigation.
