# sql-fun-sqlast

Semantic analysis for PostgreSQL and SQLite used by [sql-fun](https://crates.io/crates/sql-fun).

This crate is normally used through `sql-fun`.

## Recursive SQL analysis

On supported targets, recursive expression, query and JOIN conversion and
resolution grow the stack when needed on the calling thread. Applications do
not need to set `RUST_MIN_STACK` or reserve a 16 MiB thread stack for AdventureWorks
schema analysis. Diagnostics and inferred columns retain their usual behavior.

Stack growth is tested on `x86_64-unknown-linux-gnu`. Other targets depend on the
[`stacker` backend support](https://github.com/rust-lang/stacker#platform-support);
unsupported backends provide no stack-growth protection. Parser depth limits and
available process memory still apply. Extremely deep trees can also exhaust the
stack in operations outside the instrumented conversion/resolution paths, such
as cloning, dropping, serialization or downstream AST traversal; this is not an
unlimited-depth guarantee for every AST operation.
