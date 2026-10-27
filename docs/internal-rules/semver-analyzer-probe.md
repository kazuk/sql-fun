# SemVer analyzer capability probes (#356)

## Purpose and status

These measured results inform the release-intent policy in
[semver-release-intent.md](semver-release-intent.md) and the analyzer adapter in
later implementation issues. They are the reviewed investigation evidence
incorporated into the final, approved policy for #355; they do not establish
complete SemVer coverage or completed comparator/CI implementation. The
reproducible fixture definitions below are maintained with this report; repeat
them when changing the authoritative analyzer version or input path.

Measured on 2026-10-27 with **cargo-semver-checks 0.51.0**, rustc `1.99.1
(58cf8f9c3 2026-10-13)`, Cargo `1.99.1 (5f94df478 2026-08-27)`, and host/target
`x86_64-unknown-linux-gnu`. This is the analyzer version already pinned in
`ci/github-actions/Dockerfile`. All fixtures are dependency-free and independent
of sql-fun baseline archives, packaging, caching and CI integration. No
contributor setup or required command changes are introduced.

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
declarations to `[features]`. Both versions deliberately remain `1.0.0`: the
explicit release level controls the experiment without relying on version-number
inference. Run each row three times, replacing `patch` with `minor` and `major`:

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
unexpected statuses or missing results must also be treated as analysis
failures, never a pass. A major-level success can mean checks were skipped; it
does not establish that all surfaces were analyzed.

## Library and Cargo feature fixtures

`A` below means `pub fn api() {}`; `B` means `pub fn api() -> u8 { 1 }`. An
empty source contains no items. Sources shown in a cell are complete after
expanding these abbreviations.

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
major and taking the first successful result misclassifies ordinary additions as
patch. A changed function body passing says nothing about behavioral
compatibility. Feature inventory and requested coverage must be validated
independently of successful exit codes.

## Procedural macro fixtures and input boundary

All macro sources start with `extern crate proc_macro;`. The following
definitions supply complete source after that line:

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

Each row returns **101/101/101** through the source-manifest command above: `no
crates with library targets selected, nothing to semver-check`. A proc-macro
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
compiler/toolchain strategy. Keep compiler and JSON format compatibility under
explicit control in any later adapter.

| Case | Old definition | New definition | JSON exit codes | Observation |
| --- | --- | --- | --- | --- |
| Function-like removal | F | Empty | 100/100/0 | Macro removal is detected. |
| Function-like addition | Empty | F | 0/0/0 | Addition is not classified as minor. |
| Expansion change | F | F with body `{ proc_macro::TokenStream::new() }` | 0/0/0 | Expansion behavior is not checked. |
| Derive removal | D | Empty | 100/100/0 | Derive removal is detected. |
| Attribute removal | T | Empty | 100/100/0 | Attribute macro removal is detected. |

JSON comparison demonstrates name-level removal coverage for these three macro
kinds, not compatibility of accepted input syntax, diagnostics, helper
attributes, or generated Rust APIs. Existing macro/compiler tests remain
necessary. Do not silently skip proc-macro packages or replace their target with
a different library target. A later implementation must explicitly choose and
validate its JSON production path before relying on these results.

## JSON metadata and parser failure controls

The JSON boundary has an additional limitation. Generate JSON for both sides of
**Feature removal, unchanged API** using the rustdoc commands above, then
compare those JSON files with all three release levels. The result is **0/0/0**,
whereas the source-manifest path returns **100/100/0** with `feature_missing`.
Bare rustdoc JSON does not carry the Cargo feature inventory needed by that
check. An adapter using JSON for library or macro APIs must also compare
released Cargo feature inventories; it cannot inherit the source-path result by
assumption.

As a separate parser-failure control, replace the current JSON file with the
single byte `{`, retain the valid baseline JSON, and run the three comparisons.
The results are **101/101/101**. Malformed analyzer input is an analysis error,
independent of the requested release level.

Across the source matrix, macro JSON matrix and these two controls, this report
records **78 comparison invocations**. No external baseline or registry package
was used.

## Follow-up: shipped opt-in lint inventory

The additional verification requested in [PR #818's review
comment](https://github.com/kazuk/sql-fun/pull/818#issuecomment-6431640872) was
performed on 2026-10-27. [Upstream #949](https://github.com/obi1kenobi/cargo-semver-checks/issues/949) is a
tracking issue, not the inventory of version 0.51.0. Its unchecked public
function addition proposal must not be mistaken for an available lint.

Inspection of the [v0.51.0 shipped lint
definitions](https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints)
found **260 lints: 215 deny-by-default, 14 warn-by-default and 31
allow-by-default**. The CLI's
`cargo semver-checks --list` lists the 229 default lints; it is insufficient for
this inventory. The complete allow-by-default inventory is:

```text
auto_trait_impl_added
enum_must_use_removed
enum_no_longer_non_exhaustive
enum_non_exhaustive_struct_variant_field_added
enum_non_exhaustive_tuple_variant_field_added
enum_repr_int_added
enum_variant_no_longer_non_exhaustive
exhaustive_enum_added
exhaustive_struct_added
exhaustive_struct_with_doc_hidden_fields_added
exhaustive_struct_with_private_fields_added
feature_newly_enables_feature
function_must_use_removed
function_no_longer_unsafe
function_now_const
inherent_associated_pub_const_added
inherent_method_added
inherent_method_must_use_removed
inherent_method_no_longer_unsafe
inherent_method_now_const
non_exhaustive_enum_added
non_exhaustive_struct_added
pub_const_added
pub_static_added
repr_c_added
struct_must_use_removed
struct_no_longer_has_non_pub_fields
struct_no_longer_non_exhaustive
trait_must_use_removed
union_added
union_must_use_removed
```

All 31 have `required_update: Minor`. For each experiment below, append this
section to the **candidate** manifest (repeat the assignment for every name in
the inventory above), retaining the baseline manifest unchanged:

```toml
[package.metadata.cargo-semver-checks.lints]
exhaustive_struct_added = "deny"
# Repeat: <each of the other 30 names> = "deny"
```

Run the source-manifest comparison from the reproduction contract for patch,
minor and major. These are actual opt-in results; the original tables above
remain evidence for the default configuration.

| Case | Old Rust source | New Rust source | Exit patch/minor/major | Finding |
| --- | --- | --- | --- | --- |
| function | Empty | `pub fn api() {}` | 0/0/0 | None |
| struct | Empty | `pub struct Api;` | 100/0/0 | `exhaustive_struct_added` |
| private-struct | Empty | `pub struct Api(u8);` | 100/0/0 | `exhaustive_struct_with_private_fields_added` |
| hidden-field-struct | Empty | `pub struct Api { #[doc(hidden)] pub field: u8 }` | 100/0/0 | `exhaustive_struct_with_doc_hidden_fields_added` |
| non-exhaustive-struct | Empty | `#[non_exhaustive] pub struct Api;` | 100/0/0 | `non_exhaustive_struct_added` |
| enum | Empty | `pub enum Api { A }` | 100/0/0 | `exhaustive_enum_added` |
| non-exhaustive-enum | Empty | `#[non_exhaustive] pub enum Api { A }` | 100/0/0 | `non_exhaustive_enum_added` |
| union | Empty | `pub union Api { pub field: u8 }` | 100/0/0 | `union_added` |
| alias | Empty | `pub type Api = u8;` | 0/0/0 | None |
| trait | Empty | `pub trait Api {}` | 0/0/0 | None |
| constant | Empty | `pub const API: u8 = 0;` | 100/0/0 | `pub_const_added` |
| static | Empty | `pub static API: u8 = 0;` | 100/0/0 | `pub_static_added` |
| method | `pub struct Api;` | `pub struct Api; impl Api { pub fn api(&self) {} }` | 100/0/0 | `inherent_method_added` |
| declarative-macro | Empty | `#[macro_export] macro_rules! api { () => {} }` | 0/0/0 | None |

Type coverage is partial: structs, enums and unions have shipped addition lints;
plain type aliases, standalone functions, new traits and new macros have no
corresponding general addition lint in this version. Trait associated-item
breaking-change lints are not a new-trait detector. Proc-macro additions are
covered by the separate JSON experiments below, not by a nonexistent opt-in
macro-addition lint.

Configuration itself has an input boundary. Repeating the **struct-add** JSON
comparison below with all 31 `deny` entries in the candidate manifest, including
passing `--manifest-path` alongside `--current-rustdoc`, returned **0** for
patch. The corresponding source comparison returned **100**. The JSON path skips
manifest lint configuration, as the pinned [configuration
contract](https://github.com/obi1kenobi/cargo-semver-checks/blob/v0.51.0/README.md#implementation-details--limitations)
also states. Do not claim opt-in coverage for that path or assume merely passing
a manifest enables it.

## Follow-up: cargo-public-api differential contract

Tested **cargo-public-api 0.52.0**, installed for this investigation with:

```bash
cargo install cargo-public-api --version 0.52.0 --locked \
  --root /tmp/semver-356/public-api-tool
export PATH="/tmp/semver-356/public-api-tool/bin:$PATH"
```

This tool is absent from the development Dockerfile, contributor setup and
current user-installation prerequisites; it was not previously installed in the
builder. This is a temporary investigation prerequisite, not a new contributor
obligation. A later implementation adopting it must separately plan its pinned
build/CI provisioning under #357–#361. No product CLI runtime prerequisite is
introduced here.

Use the original dependency-free manifest. Sources in the next table are
complete, except `A`, `F`, `D`, `T` and `P` abbreviations:

- `A` is `pub fn api() {}`.
- `F`, `D`, `T` are the complete function-like, derive and attribute macro
  definitions above, prefixed by `extern crate proc_macro;`.
- `P` is just `extern crate proc_macro;` (no macro definitions).

Generate old and new JSON independently with rustc 1.99.1, using the same
target, crate name, edition and compiler. Unlike the original command, always
disable default features; add `--features <selection>` separately for each side
when the feature table specifies a nonempty selection:

```bash
RUSTC_BOOTSTRAP=1 cargo rustdoc --manifest-path old/Cargo.toml --lib \
  --no-default-features -- -Z unstable-options --output-format json
RUSTC_BOOTSTRAP=1 cargo rustdoc --manifest-path new/Cargo.toml --lib \
  --no-default-features -- -Z unstable-options --output-format json
cargo public-api --color never diff \
  old/target/doc/semver_probe.json new/target/doc/semver_probe.json
```

Both generated files have `format_version = 61`. No `--omit`, `--simplified`, or
`--include function-parameter-names` flags were used: auto/blanket/derived impls
remain visible and ordinary function parameter names are omitted. For each pair
also run:

```bash
cargo public-api --color never diff old.json new.json --deny all
cargo public-api --color never diff old.json new.json --deny added
cargo public-api --color never diff old.json new.json --deny changed
cargo public-api --color never diff old.json new.json --deny removed
cargo semver-checks check-release \
  --current-rustdoc new.json --baseline-rustdoc old.json \
  --release-type patch --color never
```

Here `old.json` and `new.json` refer to the exact generated files above, not a
regenerated or filtered API listing. Repeat the last command for minor and
major.

The public-api CLI prints three sections: removed, changed and added. A changed
item is a matched public path with different rendered declarations, shown as
`-old` and `+new` in the **changed** section; those lines are not separate
removed/added categories. Multiple impl/member lines can belong to one source
change. Default diff exits **0 even for differences**. With `--deny`, it exits
**1** when any requested category is nonempty, otherwise **0**. In the table,
`A`, `C`, `R` mean nonempty added/changed/removed categories; `–` means none.
The four denial-status digits are in **all/added/changed/removed** order. All
successful plain diff invocations below returned 0.

The classification column applies the approved conservative policy to the
measured diff and SemVer results. These are policy-required outcomes, not
implemented comparator behavior. `Patch*` means no structural finding, subject
to the coverage and behavioral limits below. SemVer codes are patch/minor/major.

| Case | Old Rust source | New Rust source | Diff | Deny statuses | SemVer JSON codes | Policy-required level |
| --- | --- | --- | --- | --- | --- | --- |
| unchanged | A | A | – | 0000 | 0/0/0 | Patch* |
| function-add | A | `pub fn api() {} pub fn added() {}` | A | 1100 | 0/0/0 | Minor |
| function-remove | A | Empty | R | 1001 | 100/100/0 | Major |
| signature | A | `pub fn api(_: u8) {}` | C | 1010 | 100/100/0 | Major |
| struct-add | Empty | `pub struct Api;` | A | 1100 | 0/0/0 | Minor |
| enum-add | Empty | `pub enum Api { A }` | A | 1100 | 0/0/0 | Minor |
| alias-add | Empty | `pub type Api = u8;` | A | 1100 | 0/0/0 | Minor |
| trait-add | Empty | `pub trait Api {}` | A | 1100 | 0/0/0 | Minor |
| trait-impl-add | `pub struct Api; pub trait Trait {}` | `pub struct Api; pub trait Trait {} impl Trait for Api {}` | A | 1100 | 0/0/0 | Minor |
| trait-required-method | `pub trait Api {}` | `pub trait Api { fn run(&self); }` | A | 1100 | 100/100/0 | Major |
| inherent-add | `pub struct Api;` | `pub struct Api; impl Api { pub fn run(&self) {} }` | A | 1100 | 0/0/0 | Minor |
| reexport-add | `mod private { pub struct Api; }` | `mod private { pub struct Api; } pub use private::Api;` | A | 1100 | 0/0/0 | Minor |
| reexport-move | `mod a { pub struct Api; } pub use a::Api;` | `mod b { pub struct Api; } pub use b::Api;` | – | 0000 | 0/0/0 | Patch* |
| reexport-alias | `pub struct Api;` | `pub struct Api; pub use Api as Alias;` | A | 1100 | 0/0/0 | Minor |
| self-spelling | `pub struct Api; impl Api { pub fn make() -> Api { Api } }` | `pub struct Api; impl Api { pub fn make() -> Self { Self } }` | C | 1010 | 0/0/0 | Outside project inputs; raw rule Major |
| param-rename | `pub fn api(first: u8) {}` | `pub fn api(second: u8) {}` | – | 0000 | 0/0/0 | Patch* |
| generic-rename | `pub fn api<T>(x: T) {}` | `pub fn api<U>(x: U) {}` | C | 1010 | 0/0/0 | Major |
| body-change | `pub fn api() -> u8 { 1 }` | `pub fn api() -> u8 { 2 }` | – | 0000 | 0/0/0 | Patch* |
| const-change | A | `pub const fn api() {}` | C | 1010 | 0/0/0 | Major |
| deprecated | A | `#[deprecated] pub fn api() {}` | – | 0000 | 100/0/0 | Minor |
| proc-add | P | F | A | 1100 | 0/0/0 | Minor |
| proc-remove | F | P | R | 1001 | 100/100/0 | Major |
| derive-add | P | D | A | 1100 | 0/0/0 | Minor |
| derive-remove | D | P | R | 1001 | 100/100/0 | Major |
| attribute-add | P | T | A | 1100 | 0/0/0 | Minor |
| attribute-remove | T | P | R | 1001 | 100/100/0 | Major |
| proc-body | F | `F` with body `{ proc_macro::TokenStream::new() }` | – | 0000 | 0/0/0 | Patch* |
| struct-remove | `pub struct Api;` | Empty | R | 1001 | 100/100/0 | Major |
| trait-remove | `pub trait Api {}` | Empty | R | 1001 | 100/100/0 | Major |
| impl-remove | `pub struct Api; pub trait Trait {} impl Trait for Api {}` | `pub struct Api; pub trait Trait {}` | R | 1001 | 0/0/0 | Major |
| reexport-remove | `mod private { pub struct Api; } pub use private::Api;` | `mod private { pub struct Api; }` | R | 1001 | 100/100/0 | Major |
| enum-variant | `pub enum Api { A }` | `pub enum Api { A, B }` | A | 1100 | 100/100/0 | Major |
| field-change | `pub struct Api { pub field: u8 }` | `pub struct Api { pub field: u16 }` | C | 1010 | 0/0/0 | Major |
| declarative-add | Empty | `#[macro_export] macro_rules! api { () => {} }` | A | 1100 | 0/0/0 | Minor |
| macro-input | `P` plus `#[proc_macro_derive(Api)] pub fn api(_: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }` | `P` plus `#[proc_macro_derive(Api, attributes(helper))] pub fn api(_: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }` | – | 0000 | 0/0/0 | Patch* |
| warning-order | `#[derive(PartialEq, PartialOrd)] pub struct Api { pub a: u8, pub b: u8 }` | `#[derive(PartialEq, PartialOrd)] pub struct Api { pub b: u8, pub a: u8 }` | – | 0000 | 0/0/0, Major warning for patch/minor | Major from warning |

For example, the function-add diff is exactly
`+pub fn semver_probe::added()`. Function-like, derive and attribute additions
render respectively as:

```text
pub proc macro semver_probe::api!()
pub proc macro semver_probe::#[derive(Api)]
pub proc macro semver_probe::#[api]
```

Struct additions include their visible auto/blanket impls.

### Representation changes and conservative outcomes

- Moving a type between private modules while preserving its public re-export
  produces no diff: the public import path is preserved.
- Returning `Self` instead of the identical concrete `Api` produces a changed
  declaration (`-> semver_probe::Api` versus `-> Self`), despite SemVer checks
  passing all levels. This Self transition is excluded by the project's
  enforced lint, as verified below. Renaming generic `T` to `U` also reports
  changed and conservatively requires Major under the policy; that is not
  evidence of an actual Cargo-incompatible change.
- Renaming a value parameter alone produces no diff under the tested flags.
- Making a function `const` reports changed and therefore Major conservatively,
  although the shipped opt-in `function_now_const` lint calls this Minor.
  Default JSON SemVer comparison cannot lower that conservative result.
- An added required trait method or exhaustive enum variant appears only in the
  added category, but SemVer findings raise the combined result to **Major**.
- Removing an impl or changing the tested public field type is caught by the
  public-api diff even though default JSON SemVer checks returned 0/0/0.
- Function body and macro expansion-body changes, and the tested derive helper
  attribute addition, produce no diff. They cannot be proven compatible by
  either structural comparator. Retain behavior/compiler tests and explicit
  review of changes outside the representation.

### Repository relevance of the Self spelling control

The workspace sets `clippy::use_self` to Warn in `Cargo.toml`, and the standard
lint step passes `-D warnings`. Confirming the **self-spelling** sources with:

```bash
cargo clippy --manifest-path old/Cargo.toml --lib -- \
  -W clippy::use_self -D warnings
cargo clippy --manifest-path new/Cargo.toml --lib -- \
  -W clippy::use_self -D warnings
```

returned **101** for the concrete `Api` spelling (two `use_self` errors) and
**0** for `Self`. The project already enforces `Self` wherever applicable.
Consequently the concrete side is not a permitted project input: this spelling
transition does not arise in repository comparisons and is not a project
classification case or an unresolved policy question. The fixture only records
the raw tool's behavior outside the project's input constraints; it creates no
requirement for normalization, lint changes or a policy exception. Generic
parameter spelling is a separate representation control.

### Warning-level findings are classification inputs

The shipped defaults are 215 Deny, 14 Warn and 31 Allow. The **warning-order**
fixture has no public-api difference, and SemVer returns 0/0/0. Nevertheless,
patch and minor emit `partial_ord_struct_fields_reordered`, with
`required_update: Major` and a warning that a new major version is suggested.
The patch summary is exactly `228 pass, 0 fail, 1 warn, 31 skip` and
`produced 1 major and 0 minor level warnings`.

The maximum-of-findings rule therefore includes warning-level
findings and yields **Major**, not Patch. A production adapter that only retries
release levels and inspects exit codes cannot implement that rule correctly.
Direct JSON input cannot promote these warnings through manifest overrides;
it must preserve and interpret their required levels, or fail validation when
it cannot establish them. This is an observed requirement for later adapter
work, not an implementation in this PR.

### Cargo feature and coverage controls

Use the sources and feature declarations below, with selections applied during
**each side's JSON generation**, not as flags on an already-built JSON diff.
`extra=[]`, `old=[]` and `new=[]` are entries in `[features]`; `–` means absent
or no selected feature. `A` retains its definition above.

| Case | Old source / declarations / selection | New source / declarations / selection | Diff | SemVer JSON codes | Policy outcome |
| --- | --- | --- | --- | --- | --- |
| feature-on | `#[cfg(feature="extra")] pub fn api() {}` / `extra=[]` / extra | `` / `extra=[]` / extra | R | 100/100/0 | Major |
| feature-off | `#[cfg(feature="extra")] pub fn api() {}` / `extra=[]` / – | `` / `extra=[]` / – | – | 0/0/0 | No finding in unselected coverage; selected released coverage still required |
| feature-add | `pub fn api() {}` / – / – | `pub fn api() {} #[cfg(feature="extra")] pub fn added() {}` / `extra=[]` / extra | A | 0/0/0 | Minor |
| feature-removed | `pub fn api() {}` / `extra=[]` / – | `pub fn api() {}` / – / – | – | 0/0/0 | Major from released feature inventory; neither JSON tool detects it |
| asymmetric-loss | `#[cfg(feature="old")] pub fn api() {}` / `old=[]` / old | `#[cfg(feature="new")] pub fn added() {}` / `new=[]` / new | AR | 100/100/0 | Major |
| coverage-hidden | `#[cfg(not(feature="extra"))] pub fn api() {}` / – / – | `#[cfg(feature="extra")] pub fn added() {}` / `extra=[]` / extra | AR | 100/100/0 | Major |
| feature-both-disabled | `#[cfg(feature="extra")] pub fn api() {}` / `extra=[]` / – | `` / `extra=[]` / – | – | 0/0/0 | No finding in unselected coverage; selected released coverage still required |
| feature-add-enabled | `pub fn api() {}` / `extra=[]` / extra | `pub fn api() {} #[cfg(feature="extra")] pub fn added() {}` / `extra=[]` / extra | A | 0/0/0 | Minor |

The asymmetric-loss case removes a previously released feature and its enabled
API while enabling a differently named candidate feature. The coverage-hidden
case removes an old unconditional API while adding a candidate-only feature.
Both preserve the old API as a removed diff item; adding new coverage does not
cancel removals. Both return SemVer 100/100/0. The complementary common/released
configuration must still be compared independently; never compare only the
intersection of feature names or drop an old configuration because its selected
feature disappeared. A previously released removed feature is Major even when
its API is empty and JSON diff is empty. A fixture with both sides disabling a
feature demonstrates precisely why successful unselected comparison is not
coverage of its released enabled API.

### Input and tool failure controls

Both tools consumed the **same unmodified format-61 JSON pairs** in every
successful library, feature and proc-macro row. This establishes a usable shared
input boundary for these fixtures under the recorded compiler, not blanket
compatibility with any future rustdoc version or an unchosen archive format.

A separate format-55 control was generated for `pub struct Api;` with
`nightly-2025-08-02` (`rustc 1.90.0-nightly (4b55fe199 2025-08-01)`), installed
in an isolated temporary `RUSTUP_HOME`:

```bash
RUSTUP_HOME=/tmp/semver-356/rustup \
  rustup toolchain install nightly-2025-08-02 --profile minimal
RUSTUP_HOME=/tmp/semver-356/rustup \
  rustup run nightly-2025-08-02 cargo rustdoc \
  --manifest-path new/Cargo.toml --target-dir format55 --lib \
  --no-default-features -- -Z unstable-options --output-format json
```

| Control | public-api result | SemVer result | Interpretation |
| --- | --- | --- | --- |
| Format 55 compared with itself | 1, parse error (`missing field path`) | 101, unsupported format | Old JSON is not automatically reusable. |
| Format 55 baseline versus format 61 candidate, identical source | 1, parse error | 101, unsupported format | No SemVer verdict; do not classify failed parsing as an API change. |
| Copy format-61 JSON and change only `format_version` to 999 | 0, no diff even with `--deny all` | 101, unsupported format | public-api does not validate this format marker; explicit validation is necessary. |
| Current JSON is the single byte `{` | 1, parse error | 101 | Malformed input is validation failure. |
| Current JSON path does not exist | 1, read error | 101 | Missing artifact is validation failure. |
| JSON generation requests nonexistent feature `old` in asymmetric-loss candidate | Generation exits 101 (`does not contain this feature`); no comparison | Comparison not invoked | Missing requested coverage fails validation. |
| Candidate source contains `compile_error!("probe failure");` | JSON generation exits 101; comparison not invoked | Comparison not invoked | Stop before using a missing or stale output. |

The format-55 test also shows that public-api's diagnostic suggesting a minimum
nightly date is insufficient to choose compatible artifacts. SemVer 0.51.0's
actual error lists supported schemas **57, 60, 61**; the installed public-api
0.52.0 resolves `public-api 0.52.0` and `rustdoc-types 0.57.3` in its shipped
Cargo.lock, yet accepted these format-61 fixture shapes. Acceptance is measured,
not inferred from that dependency's version. Validate compiler/schema provenance
explicitly, keep both sides on the same recorded producer, and re-probe on tool
or compiler changes. Archived JSON from an incompatible producer requires an
explicit regeneration/migration decision; silently changing or selecting a
baseline is not authorized.

Do not interpret public-api exit 1 alone as a compatibility result: denied diffs
and parser/read failures share that status. A successful plain diff or
structured library result is required before interpreting its categories. Tool
failure, unknown exit status, incompatible format, or failed JSON generation is
**validation failure**, never a successful pass or a Major SemVer verdict.

The follow-up records 405 comparator invocations: 42 source opt-in checks,
352 comparisons across 44 successfully generated JSON pairs (five public-api
modes and three SemVer levels each), and 11 input/configuration controls.
Two additional generation-failure cases were checked without invoking a
comparator; two Clippy runs establish the repository relevance of the Self
control. The original 78 comparisons remain recorded above.

## Approved classification contract

The following approved repository policy applies within explicitly validated
structural coverage. This investigation records its evidence; comparator and
CI implementation remain separate work:

1. No structural difference starts at Patch; additive-only API diff starts at
   Minor; any removed/changed item or successfully observed but unclassifiable
   API difference starts at Major.
2. Take the maximum of that result and the required level of applicable
   cargo-semver-checks findings, including warning-level findings. This is
   essential for added enum variants and
   required trait methods. No finding can lower a conservative diff result.
3. Compare released Cargo feature inventories separately. Removal requires
   Major; a new opt-in feature introduces new coverage and does not hide old
   coverage. Ordinary added APIs still require Minor under this policy.
4. Compare every previously released selected API surface plus new coverage;
   retain explicit old/new selections and reject missing intended coverage.
5. Validate tool/JSON/compiler provenance and successful completion before any
   classification. Analysis failure fails validation, rather than becoming a
   SemVer category. Unanalyzed behavioral/macro-input changes remain explicitly
   outside structural proof and require complementary review/tests.

No successful output in this matrix had a fourth, unclassifiable diff category.
The Major rule for an observed but unclassifiable API difference is an approved
policy boundary, not a measured extra public-api category. The parser-failure
controls specifically exercise validation failure instead.

This is conservative about **observed differences**, including known harmless
representation changes; it is not a completeness theorem for all Rust or macro
behavior. It deliberately trades false Major requirements for a simple,
reviewable boundary without semantic normalization heuristics. That tradeoff
and the complementary review boundary are approved repository policy. An empty
structural diff must not silently clear a known out-of-model compatibility
concern.

The reproducible matrices establish the approved combination for the tested
cases and supply the reviewed #356 findings incorporated into #355's final
specification. They remain investigation evidence: comparator implementation,
CI integration, contributor procedures and baseline migration are separate work
and are not implemented by this documentation change.
