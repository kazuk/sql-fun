# Code generator guidelines

Read this document before planning, implementing, or reviewing changes to Rust
code generators and templates. It supplements the
[Rust coding guideline](rust-coding-guideline.md) with rules for code that
executes in a caller's context.

## Make generated name resolution deliberate

Generated code must not depend on incidental imports, names, or inherent methods
in the caller's code.

- When the generator intends to invoke a specific trait operation, use a
  qualified trait call. Method syntax can select a user-defined inherent method
  with the same name.
- Use absolute paths for generator-owned references where the dependency
  contract permits them, such as `::core::convert::Into`. Respect documented
  configurable paths and dependency names; do not replace those choices with
  hard-coded assumptions.
- Allocate generated local identifiers through the generator's existing
  collision-avoidance mechanism. A distinctive prefix alone does not prevent a
  caller from using the same name.
- Preserve deliberate calls to user-provided methods, such as configured
  parameter getters or row-builder setters. Qualify generator-owned trait
  operations, not every method call.

For example, when an interrupt future supplies an error that must be converted
into the function's declared error type, generate:

```rust
return Err(::core::convert::Into::into(interrupt_result));
```

Avoid generating `interrupt_result.into()` for that conversion. A caller's error
type may define an inherent `into()` method returning an unrelated type, even
when it also implements the required conversion trait. The method call can then
fail to compile or select unintended behavior. An absolute type path does not
by itself disambiguate method calls on values of that type.

## Keep error conversions at their intended boundaries

Review the types of generated branches before relying on an outer `?` or return
type to convert their errors. Branches that produce a shared expression must
agree on its type before a later conversion can occur.

For PostgreSQL execute interruption, distinguish:

- Conversion of the interrupt future's output into the declared function error.
- Conversion of `tokio_postgres::Error` from the execute operation through `?`.

Putting both results inside the same `tokio::select!` expression can force the
interrupt error to have the driver's type before `?` is applied. Returning the
converted interrupt error from the enclosing function avoids that unnecessary
constraint. Check the enclosing generated function or helper: `return` must
target the intended public result boundary.

## Test the caller-visible failure with a focused regression

A generator building successfully does not prove that its output compiles or
behaves correctly in a caller's context.

- Reproduce the failure through the public macro or generation entry point.
  Prefer compilation and behavior checks over assertions on token formatting.
- For a trait-method collision, use a fixture with the conflicting inherent
  method and the intended trait conversion. Verify that generated code compiles
  and that the trait conversion returns the intended value.
- Preserve concrete accepted signatures affected by the change, including
  native and wrapped error combinations when relevant.
- Exercise applicable generation modes using the existing harness. Metadata or
  CHECK-mode stubs alone cannot validate executable generated code.
- Keep coverage tied to the changed behavior. Do not add a broad matrix for
  unchanged success and error paths merely because they share a generator.
- Use deterministic futures for interruption behavior tests rather than sleeps
  or a live database when the existing fixture can model the operation.

Use the repository-standard validation steps covering the changed files and
review the generated operations manually for name resolution and type
constraints. Existing code is not evidence that an unqualified operation is safe.

The conversion example comes from the review of
[PR #353](https://github.com/kazuk/sql-fun/pull/353#discussion_r4180562153),
which fixes [issue #227](https://github.com/kazuk/sql-fun/issues/227).
