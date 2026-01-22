---
name: implement-unit-test
description: |
  Implement Rust unit tests for a specified source file.
  Use when the user provides coverage-driven target files.
---

# Implement unit test

 Add unit tests to specified source, and run.

## Required parameters
The user must provide parameters in one line:

- source=<path>

## Missing parameters
- If `source` is missing, ask only for `source` and proceed.

## Procedure

1. Open `source` and identify testable units.
2. Design test input and design helper function.
3. Implement unit tests.
4. MUST Run `check-test-flow` script for test verification.
5. If `check-test-flow` fails, fix and re-run until it passes.
6. Finaly run `update-stats` for update coverage statistics.

## Execution rules

- If you modify somewhat MUST run tests with `check-test-flow`.
- If struct or enum `impl` block contains multiple `fn`. separete to per-item `impl` blocks
  ```rust
  impl SomeWhat {
    fn a() { ... }
    fn b() { ... }
    fn c() { ... }
  }
  ```
  to be
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
  - See `docs/internal-rules/rust-coding-guidline.md#test-placement`

- Use crate-level `test_helpers` module for helper functions/types.
  - If `src/test_helpers.rs` does not exist, create it.
  - Ensure `src/lib.rs` contains:
    ```rust
    #[cfg(test)]
    pub mod test_helpers;
    ```

- Extract helpers into `crate::test_helpers` to keep tests simple, BUT:
  - Do NOT move test-case-specific inputs/expected values into `test_helpers` as hardcoded constants.
  - `test_helpers` must be generic and parameterized:
    - Provide builders/parsers/assert helpers that take inputs as parameters.
    - Prefer `fn make_* (..params..)` / `fn parse_* (input: &str)` / `fn assert_* (actual, expected)` style helpers.
  - Test case data (inputs and expected outputs) must stay in the test module:
    - Keep them inline in each test, or
    - Use table-driven tests in the test module (arrays/vec of cases) and pass each case into helpers.
- When function seems low-testability.
  - plan refactor for testability and Add TODO comment like bellow.
    `// TODO: TESTABILITY: extact for loop inner to function`
- Use `check-test-flow` script for test compilation and run.
- After tests pass, run `./.local/bin/update-stats` to update coverage statistics.
