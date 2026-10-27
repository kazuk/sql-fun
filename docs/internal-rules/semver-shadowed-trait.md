# Identical-input trait replacement false positive (#826)

## Consumer and boundary

This investigation supports the package comparator from PR #821, the lifecycle
coordinator in PR #824, and release-intent enforcement in #359/#361. It does
not change release policy or baseline selection. The repair is a corrected
upstream lint query, not a second classifier or a post-processing exception.

The comparator requires **cargo-semver-checks 0.51.0-sqlfun.1**. The internal
preparation helper builds this executable from the upstream 0.51.0 source
archive, verified against SHA-256
`484e65e62077aed1a119d04ab414038d20b9013302b7353487ff69a461842bf8`.
The archive records upstream revision
`260fad71517e8b320c5c1ad1f367900ab37362a1`. It retains the upstream package
version and dependency lockfile. Only the CLI version output receives a
distinct identity.
Upstream lint documentation links continue to reference the real 0.51.0 tag.
No compiler substitution, lint disabling or CI image adoption is involved.
Future upstream upgrades must re-probe the correction and the existing
analyzer contracts before replacing this build.

## Reproduction and cause

Approved source revision: `bd53b083b028dc8423f6c4e5b232cbd3c851c8c1`.
The baseline from #824 revision `7ddd5c30fc770aa40e7fba0844a3edd7b4aa7fa2`
has SHA-256
`7985670a095dc6b696edc471f9dbfdbc5947f7d5fcebcb960e68412b0cf91cec`.
It records default-enabled `postgres` coverage for ten packages, nine of which
have library/proc-macro APIs; `adventure-works` is binary-only.

Rust/Cargo 1.99.1 on `x86_64-unknown-linux-gnu`, rustdoc schema 61,
`public-api 0.52.0` and stock cargo-semver-checks 0.51.0 reproduce the
`sql_fun_sqlast::syn::ListOpt` Major finding with an empty structural diff.
The finding also occurs when comparing **one JSON file to itself**, for both
independently generated files, and when reversing the old/new pair. This rules
out independent-generation differences or comparator argument ordering as the
cause of this finding.

A minimized source fixture deliberately uses glob re-exports to reproduce the
historical input:

```rust
mod traits {
    pub trait Api {}
}
mod generated {
    mod inner {
        pub struct Api(Option<u8>);
    }
    pub use self::inner::Api;
    pub fn keep(_: Api) {}
}
pub mod api {
    pub use crate::traits::Api;
    pub use crate::generated::*;
}
```

The signature keeps the otherwise shadowed tuple type in rustdoc's index.
The type has a private field, represented by `None` in its tuple field list.
The pinned analyzer's adapter nevertheless supplies a struct import path that
matches the trait path. A unit struct also demonstrates a related valid case:
its value can share a textual path with a trait in the type namespace.
Textual path equality alone does not establish that a trait was replaced.

The [upstream 0.51.0
query](https://github.com/obi1kenobi/cargo-semver-checks/blob/v0.51.0/src/lints/trait_changed_kind.ron)
finds a baseline trait and any candidate struct, enum or union at the same
textual import path. It does not check whether the candidate still exports the
trait there. Consequently an unchanged input can satisfy that query.

## Correction and retained guarantees

Before reporting a kind change, the corrected query requires **zero candidate
public-API traits at the baseline path**. It uses the same folded count/zero
predicate already used by upstream
[`trait_missing`](https://github.com/obi1kenobi/cargo-semver-checks/blob/v0.51.0/src/lints/trait_missing.ron).
All replacement-kind matching, default severity, output fields and other lints
remain intact. The query runs in the analyzer before findings are produced;
the comparator still consumes every denied and warning-level finding.

This condition is path-specific: preserving a trait under another alias does
not excuse replacement at its old path. A trait becoming hidden from public
API does not satisfy the public-API presence condition either. Existing method,
bound, safety, visibility and other trait lints continue to analyze retained
traits. This repair does not claim to repair every upstream import-path issue.

Both analyzers still parse the same fresh JSON. Schema/producer/coverage checks,
removed-feature handling and fail-closed textual output validation remain
required. Stock 0.51.0 is rejected by the executable identity check; matching
source inputs or an empty structural diff never bypass analysis.

## Maintained verification and evidence

`tests/package-api/test_compare.py`, invoked by the existing `xbuild` integration
test through `workspace` and nextest, owns:

- unchanged shadowed trait/private-tuple and trait/unit-struct controls;
- actual trait-to-struct, enum and union replacements;
- replacement at one path while retaining the trait under a different alias;
- complete approved-baseline self-comparison, requiring nine Patch results and
  explicitly checking the binary-only exclusion;
- existing Major warnings with exit zero, required trait methods, exhaustive
  variants, features, proc macros, producer drift, unsupported schemas and
  analysis failures.

`baseline_helpers.py` extracts the frozen test fixture
`tests/package-api/fixtures/issue826-baseline.tar.gz` and checks its checksum.
This is a byte-identical copy of the approved seed above, retained independently
of future active-baseline advancement. It prepares historical registry
prerequisites outside the production comparison. This gives the full replay a
maintained execution path without depending on #824 being merged or changing
the repository's active baseline. The standard contributor command is unchanged.

The repair PR records full replay results, exact tool identities and retained
raw JSON/log evidence. These generated investigation artifacts are not permanent
source fixtures; the maintained tests regenerate the relevant surfaces.
