# `sql-fun-codegen`

Type definitions for Rust AST.

## Reading unresolved queries

`QueryRequirements::extract(&syn::File)` reads unresolved connector stubs from
expanded Rust source. Each requirement includes its datasource, encoded query,
source location, and a deserialized `ResolvedCteCatalog`.

The stub layout and its reader are a private protocol of this crate. Only a
`__SQL_FUN_CODEGEN_VERSION` constant directly inside a function identifies a
stub. Without it, the reader continues normal recursive AST traversal. Once a
stub is identified, missing, duplicate, or invalid required constants are
errors. The reader expects macro expansion to have replaced `line!()` with its
literal.

The embedded `sql-fun-codegen` version must exactly match the reader's version;
this is checked before deserializing CTE information. Protocol changes require a
codegen version change. The `sql-fun` proc-macro and `sql-fun-cli` package
versions can evolve independently, provided they use the same codegen version.
There is no fallback parser for older or newer stub formats.
