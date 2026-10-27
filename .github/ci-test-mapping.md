# CI test mapping

This document describes which parts of the test suite are exercised by the main
CI jobs and why.

## Test responsibilities

| CI job | Feature set | Test scope | Responsibility |
| --- | --- | --- | --- |
| `workspace` | default features | workspace crates except the root `sql-fun` crate | Compile and test internal crates with optional features disabled. |
| `workspace-all-features` | all features | workspace crates except the root `sql-fun` crate | Compile and test internal crates with optional features enabled, including SQLite-specific logic. |
| `proc-macro-tests` | all features | root `sql-fun` crate | Run integration/E2E tests for generated code, connectors, metadata, and proc-macro behavior. |

## Why the root crate is excluded from workspace tests

The root `sql-fun` crate is intentionally a thin proc-macro integration layer.
Most implementation logic lives in crates such as `sql-fun-codegen`,
`sql-fun-core`, and `sql-fun-sqlast`, where unit and crate-level tests exercise
behavior directly.

Tests under the root crate are therefore treated as integration/E2E tests. They
validate that the proc-macro entry points, generated code, metadata, and
connector-specific paths work together.

Because these tests are comparatively expensive, CI runs the root test suite
once with all features enabled rather than duplicating the E2E suite for each
feature combination.

Feature-dependent implementation logic is still exercised in both directions by
the two workspace jobs:

- `workspace` covers the default, SQLite-disabled configuration.
- `workspace-all-features` covers the SQLite-enabled configuration.

This split is intentional. If root-level E2E behavior ever becomes
feature-dependent in a way that cannot be covered by the all-features run, the
CI mapping should be revisited.
