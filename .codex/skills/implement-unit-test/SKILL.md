---
name: implement-unit-test
description: |
  Implement Rust unit tests for a specified source file.
  Use when the user provides coverage-driven target files.
---

# Implement unit tests

Add unit tests to the specified source file and run them.

## Required parameters

The user must provide parameters in one line:

- `source=<path>`

## Missing parameters

- If `source` is missing, ask only for `source` and proceed.

## Procedure

1. Open `source` and identify testable units.
2. Design test inputs and helper functions.
3. Implement unit tests.
4. Run the required `check-test-flow` script to verify the tests.
5. If `check-test-flow` fails, fix and re-run until it passes.

## Execution rules

- If you make any changes, you MUST run tests with `check-test-flow`.
- If a struct or enum `impl` block contains multiple functions, separate them
  into per-item `impl` blocks:

  ```rust
  impl SomeWhat {
    fn a() { ... }
    fn b() { ... }
    fn c() { ... }
  }
  ```

  becomes:

  ```rust
  impl SomeWhat {
    fn a() { ... }
  }
  impl SomeWhat {
    fn b() { ... }
  }
  impl SomeWhat {
    fn c() { ... }
  }
  ```

- Place tests immediately after the implementation block or function they cover.
  - See `docs/internal-rules/rust-coding-guideline.md#test-placement`
    (relative to the repository root).
- Use a crate-level `test_helpers` module for helper functions/types.
  - If `src/test_helpers.rs` does not exist, create it.
  - Ensure `src/lib.rs` contains:

    ```rust
    #[cfg(test)]
    mod test_helpers;
    ```

  - Same-crate unit tests can use `crate::test_helpers`; expose shared helper
    functions/types as `pub(crate)` where needed.
  - Integration tests under `tests/` compile as separate crates and cannot use
    this `#[cfg(test)]` module, because the library is compiled without
    `cfg(test)` for those consumers.
- Extract helpers into `crate::test_helpers` to keep tests simple, BUT:
  - Do NOT move test-case-specific inputs/expected values into `test_helpers` as
    hardcoded constants.
  - `test_helpers` must be generic and parameterized:
    - Provide builders/parsers/assert helpers that take inputs as parameters.
    - Prefer `fn make_* (..params..)` / `fn parse_* (input: &str)` /
      `fn assert_* (actual, expected)` style helpers.
  - Test case data (inputs and expected outputs) must stay in the test module:
    - Keep them inline in each test, or
    - Use table-driven tests in the test module (arrays/vec of cases) and pass
      each case into helpers.
- When a function is difficult to test:
  - Plan a refactoring for testability and add a TODO comment such as:
    `// TODO: TESTABILITY: extract the loop body into a function`
- Use the `check-test-flow` script to compile and run tests.
