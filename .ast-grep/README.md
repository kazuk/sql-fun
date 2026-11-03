# Rust import-path lint

The existing `check-test-flow` lint step invokes `.local/sbin/check-rust-imports`.
It requires ast-grep 0.51.1, verifies the checked-in syntax fixtures, then scans
all tracked and non-ignored untracked Rust sources in the checkout. Changed-only
build selection does not narrow this scan. Git supplies the source list without
recursing into submodules; generated SQL syntax, the vendored SQLite parser and
build outputs are excluded explicitly. Keep exclusions in the shared helper so
local and CI execution use the same boundary.

`sgconfig.yml` owns rule and fixture discovery. Use the standard lint step for
validation. The fixtures check permitted and rejected syntax; snapshots pin the
first diagnostic's span. Separate examples cover generic arguments and impl
targets so a diagnostic on one cannot hide a missed diagnostic on the other.
These small rule fixtures run in the existing lint path, without compiling Rust
or starting an independent CI job.

The coding policy and its syntax limitations belong in the
[Rust coding guideline](../docs/internal-rules/rust-coding-guideline.md#importing-items).
The rules intentionally have no automated fix: inserting imports requires
scope, conditional-compilation and name-collision decisions. In particular,
shared integration helpers mounted at different module paths should expose a
consistent crate-root alias rather than hard-code their incidental nesting.
