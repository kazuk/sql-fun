# `sql-fun-codegen`

This published crate is an internal implementation component used by the
[`sql-fun`](https://crates.io/crates/sql-fun) proc-macro and CLI. Its interfaces
are implementation details; external API compatibility is not promised.
Applications that want SQL-to-function code generation should use `sql-fun`
instead of depending on this crate directly.

Maintainers can consult the [expanded-stub protocol notes](https://github.com/kazuk/sql-fun/blob/main/docs/design-docs/codegen-expanded-stubs.md).
