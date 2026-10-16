# Rust coding guidelines for `sql-fun`

## Principle: Minimal Work, Maximal Maintainability

This coding guideline exists
**to minimize developer effort while keeping code clean**. It is not a
collection of idealistic rules that inflate implementation cost. Prefer the
solution that yields **the fewest moving parts** and
**least opportunity for dirt** over time.

### What this implies

- **Public surface = smallest necessary.**  
  Expose only types and functions; no need to access fields/variants and
  internal shapes.
- **Prefer simple patterns over ceremony.**  
  If a rule increases boilerplate without clear safety or clarity gains, don’t
  apply it.
- **Pay once, benefit always.**  
  Choose designs that reduce future edits (single source of truth, stable
  signatures).
- **Opt for standard derivations and conventions.**  
  Use `#[derive(...)]`, `From/TryFrom`, and newtype patterns to avoid ad-hoc
  glue code.
- **Localize decisions.**  
  Keep logic with the type/enum it belongs to (methods/impls over external
  `match`es).
- **Stable callers, flexible innards.**  
  Maintain constructor/ctor-like function signatures; feel free to change
  internals.
- **No premature abstraction.**  
  Don’t add traits/layers unless they demonstrably reduce call-site complexity
  or churn.

### Review rubric

- Does this change **reduce** future edit points (fewer files/places to touch)?
- Does it **shorten** call-site code or make it more self-documenting?
- Does any extra boilerplate deliver tangible safety/clarity? If not, remove it.
- Could the same effect be achieved with a **private field + ctor + method**
  instead of exposing structure?
- Will this remain easy to refactor without breaking public signatures?

> Aim for the **lowest hand-count** implementation that keeps the codebase hard
> to dirty and easy to change.

## Unimplemented functionality

- `todo!()` is Rust's standard-library macro for explicitly marking
  unimplemented functionality. It is the minimal, correct expression of that
  state, not a design defect to clean up.
- **Do not replace `todo!()` merely to express "not implemented" in another
  way.** Adding a dedicated error type, wrapper, abstraction, or fallback solely
  for that purpose is wasted work: it adds code to maintain without implementing
  any functionality.
- Preserve existing `todo!()` calls and their behavior during refactoring unless
  implementing the missing functionality or changing that behavior is explicitly
  within the task's scope. Replacing the marker is not progress toward
  implementing the feature.

## module structure

Basic structure defined in bellow.

```rust

// all separated file sub module list in head of module 
mod sub_module1;
mod sub_module2;

// re-exporting symbol definitions
//   `self::` specifier is required
pub use self::sub_module1::{SomeWhat, OtherSomeWhat};

// importing item from sub modules
//   `self::` specifiter is required
use self::sub_module2::{Foo,Bar};

// crate internal import
//   `crate::` specifier is required
use crate::foo::bar::{Baz};

// imports from `std` 
use std::io::Error as IoError;
// external create import `::crate-name` required
use ::url::Url;

// item definitions goes here.
// - function
// - enum
// - struct
// - inline module 

```

### Module file paths

- Do not use the legacy `mod.rs` module layout. Use self-named module files
  such as `foo.rs` instead of `foo/mod.rs`. This applies to production code,
  tests, and test helper modules.
- The workspace `clippy::mod_module_files` lint enforces this rule for source
  modules, but exempts integration-test paths. Check test module layouts during
  review as well.
- Follow Rust's default module file resolution conventions. Place module files
  where a plain `mod name;` declaration resolves them without an explicit path.
- Do not add `#[path = "name.rs"]` when Rust already resolves that file by
  default. This applies to test modules as well as production modules.
- Use `#[path]` only when a deliberately non-standard module location is
  required; do not use it merely to restate the default path.

### Test placement

- Place tests
  **immediately after the implementation block or function they cover**.  
  Co-locating tests with the code they verify reduces the risk of tests being
  orphaned or forgotten when modules are split or reorganized. Keep the test
  module focused on the nearby implementation; broader integration tests belong
  in dedicated test modules/files.
- Test code SHOULD stay at roughly the same abstraction level as the behavior
  under test.  
  If a unit test needs repeated low-level setup such as writing fixture files,
  assembling TOML strings, or enumerating environment variables, move that setup
  behind domain-oriented helpers in a `test_helpers` module.
- Repeated setup in tests MUST be expressed in terms of the test's intent, not
  the storage or transport details.  
  Prefer helpers such as `write_metadata_project(...)` or `TestMetadataEnvs`
  over open-coded `write_file`, raw `HashMap` assembly, or large inline fixture
  literals at each call site.
- When a test reads like filesystem choreography or key-value plumbing instead
  of a specification of behavior, treat that as a design smell in the test
  code.  
  Raise the abstraction in `test_helpers` until the test body primarily
  communicates the scenario, action, and assertion.

```rust
impl Foo {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
}

#[cfg(test)]
mod test_foo_new {
    #[test]
    fn constructs_foo_with_value() {
        let foo = super::Foo::new(10);
        assert_eq!(10, foo.value);
    }
}
```

```rust
#[cfg(test)]
mod test_metadata_args {
    use crate::test_helpers::{TestMetadataEnvs, write_metadata_project_with_cte};

    #[test]
    fn applies_metadata_related_environment_values() {
        let fixture =
            write_metadata_project_with_cte(&project_dir, "fixture", "qa", "cte")
            .expect("metadata fixture");
        let envs = TestMetadataEnvs::new()
            .with_metadata_project_with_cte(&fixture, "qa")
            .with_sql_dialect("postgresql")
            .into_map();

        // test body now describes behavior instead of setup mechanics
    }
}
```

### String literals in tests

- Choose string literal syntax to make the behavior under test apparent.
  When testing structured text such as JSON or SQL, prefer raw string literals
  (`r#"..."#`) over ordinary strings cluttered with escaped quotes or
  backslashes. Preserve the text's readability at the call site.
- Use ordinary string literals with explicit escape sequences (`\n`, `\r`,
  `\t`, `\0`, etc.) when the control characters themselves affect the test
  conditions or expected result.
- When both structured content and control characters matter, keep the content
  in a raw string literal and express the relevant control characters
  separately, for example with `concat!`. This makes the framing condition
  distinguishable from the payload.
- This is a readability rule, not a requirement to externalize fixtures or
  rewrite harmless literals. In particular, retain exact bytes and malformed
  input when their representation is part of the test.

```rust
// Test the JSON payload as readable text.
let response = r#"{"jsonrpc":"2.0","id":0,"result":"OK"}"#;

// Test that the same payload ends with a newline.
let framed_response = concat!(
    r#"{"jsonrpc":"2.0","id":0,"result":"OK"}"#,
    "\n",
);
```

### Test helper cost

- Place helpers that perform costly initialization or processing in a
  `test_helpers::heavy` submodule. This also applies to helpers that incur that
  cost indirectly by calling other helpers. Classify the processing cost of
  using the helper in a fresh test process, not the fixture's data size. Examples
  include parsing the full builtin schema or starting an external service.
- Keep the cost visible at the call site. Do not re-export heavy helpers from
  `test_helpers` or hide them behind ordinary wrappers or aliases. Import the
  `heavy` module and call `heavy::helper(...)`, or retain `heavy` in the qualified
  path when using a helper or fixture type.
- Briefly document each heavy helper's costly work, the behavior that requires
  it, and whether initialization is repeated or its result is shared. When
  sharing results, specify the scope of sharing, such as within one process or
  across processes. Callers should not need to inspect the implementation to
  understand this cost contract.
- Account for [nextest's process-per-test execution model](https://nexte.st/docs/design/how-it-works/).
  Process-local caches such as `OnceLock` or `LazyLock` do not amortize
  initialization across independent test cases, including cases generated by
  `rstest`. A process-local cache alone is not a reason to classify a costly
  helper as lightweight.
- Use lightweight, domain-oriented helpers when they can verify the intended
  behavior. Use heavy helpers when the real catalog, service, or other costly
  setup is part of what the test verifies, and briefly state why it is needed in
  the test. Do not inherit a heavy setup solely because a neighboring test uses
  it. Heavy helpers are not prohibited, and grouping independent cases into one
  test solely to share initialization is not a substitute for appropriate setup.
- This convention does not require strict execution-time thresholds, per-test
  cost measurements, or repeated investigations of fixture internals. Maintain
  the cost classification and contract with the helper rather than asking each
  caller to rediscover them.

### Compile-time test cost

- Treat `trybuild` as a high-cost compiler-test harness, not a routine way to
  assert that an API cannot be misused. Compiling test fixtures and their
  dependencies can dominate the cost of a small set of negative cases;
  snapshot maintenance adds further work. Do not introduce or copy a harness
  merely because another crate already uses one.
- Prefer [Rustdoc `compile_fail` examples](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html#compile_fail)
  for public API boundaries such as ownership, visibility, and unavailable
  mutation methods when compilation rejection is the required guarantee.
  Include a positive compiling example and keep negative examples minimal so
  unrelated setup errors do not satisfy the test. Rustdoc still has compilation
  costs; it is not a promise of cost-free validation.
- Use [trybuild](https://docs.rs/trybuild/1.0.121/trybuild/) when its diagnostic
  snapshots or other harness-specific capabilities are part of the requirement,
  such as checking a procedural macro's error message and source location.
  Briefly state why those capabilities are needed; preserve meaningful coverage
  rather than replacing diagnostic assertions with generic compilation failure.
- Give compile-time tests a maintained local/CI execution path. Rustdoc examples
  require an explicit `cargo test --doc` invocation for the relevant crate and
  feature configurations; `cargo doc` and nextest alone do not execute them.
  When evaluating cost, distinguish Cargo preparation/dependency compilation
  from test execution. Do not require per-test timing measurements or assume
  that changing harnesses eliminates the underlying compilation work.

### Parameterized tests with `rstest`

- Prefer `#[rstest::rstest]` when independent input examples exercise the same
  behavior. Each example should be a separate test case so the test runner can
  report, filter, and rerun it individually. Avoid hiding independent examples
  inside a loop in one `#[test]` function.
- Use `#[case]` for related inputs and expected results. Give cases meaningful
  names, such as `#[case::empty(...)]`, when they help identify the scenario.
- Use `#[values(...)]` for independent dimensions whose combinations all need
  coverage. Multiple value parameters generate a Cartesian product; keep the
  dimensions focused and use explicit cases when only selected combinations
  are meaningful.
- Keep loops when iteration is part of one scenario, such as applying a
  sequence of state transitions or checking every item in one result. Keep
  assertions for that scenario together; separate unrelated behaviors into
  focused tests.
- Use the qualified macro path rather than importing `rstest`, following the
  macro import rule below. Parameterization does not change the test placement
  or domain-oriented setup requirements above.

Related inputs and expectations belong in named cases:

```rust
#[rstest::rstest]
#[case::zero("0", 0)]
#[case::positive("42", 42)]
fn parses_integer_text(#[case] input: &str, #[case] expected: u32) {
    assert_eq!(input.parse::<u32>().unwrap(), expected);
}
```

Independent dimensions can use values to cover every combination:

```rust
#[rstest::rstest]
fn parses_integer_with_surrounding_whitespace(
    #[values("", " ", "\t")] leading: &str,
    #[values("", " ", "\t")] trailing: &str,
) {
    let input = format!("{leading}42{trailing}");
    assert_eq!(input.trim().parse::<u32>().unwrap(), 42);
}
```

### Importing items

- `*` import is prohibited. (including `prelude` import)
- macro importing is prohibited. (including derive / proc-macro)  
  Do NOT!

    ```rust
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct SomeWhat(String)
    ```

  Good

    ```rust
    #[derive(serde::Deserialize)]
    pub struct SomeWhat(String);
    ```

- importing just `Error` MUST be rename.

    ```rust
    use std::io::Error as StdIoError;
    ```

  prefer

    ```rust
    fn file_io_function() -> Result<SomeWhat, std::io::Error>
    ```

- importing `Result` is prohibited.
  Do NOT!

    ```rust
    use anyhow::Result;
    ```

## for `lib.rs`

- Require lint configurations
  - `#![deny(missing_docs)]`
- In principle, `pub mod` is prohibited.  
  Symbols should be re-exported from the crate’s top level,  
  except in cases where it is clearly beneficial for users to see separate
  modules —  
  for example, when defining distinct concepts such as `syn` / `sem` that would
  otherwise conflict.

## Struct definitions

- Public fields in `struct` are prohibited.  
  All access must go through constructors, getters, or methods to maintain
  encapsulation and allow future changes without breaking the public API.
- Constructors
  - For single-parameter cases, it is recommended to implement `From` or
    `TryFrom` instead of providing a constructor.
  - SHOULD take parameters by reference.  
    If ownership is required, perform `copy`/`clone` inside the constructor and
    store it in `self`.

    DO NOT!

    ```rust
    fn new( value: String ) -> Self { Self(value) }
    ```

    Good

    ```rust
    fn new( value: &str ) -> Self { Self(value.to_string()) }
    ```

- Getter method
  - DO NOT prefix `get_`.
  - SHOULD return a reference to the item, except for `Copy` types.
  - Always consider **whether a getter is truly necessary**.  
    If the struct already provides higher-level operations that combine reading
    and updating, exposing a separate getter may be unnecessary.
- Setter method
  - Prefix with `set_`
  - SHOULD take parameters by reference. If ownership is required, perform
    `copy`/`clone` inside the setter and store it in `self`.
  - Always consider **whether a setter is truly necessary**.  
    If the struct already provides domain-specific operations that cover the
    update, exposing a generic setter should be avoided.

## Enum definitions

- **Provide per-variant constructors**, and use them to create values outside
  the type's implementation instead of constructing variants directly. Keep
  constructor signatures focused on semantic inputs, rather than requiring
  callers to construct internal payload types.
  - The purpose is to keep callers independent of the internal representation,
    not merely to standardize construction syntax. For example, changing
    `enum SomeWhat` into `struct SomeWhat(SomeWhatPayload)` should not require
    rewriting callers if its constructors and methods retain the same interface.
- Having a `match` on an `enum` outside of the `enum` itself is a
  **code smell**.  
  Whenever possible, logic depending on enum variants should be encapsulated
  within methods or trait implementations defined for the enum. This also
  applies to variant destructuring with `if let`, `let ... else`, and
  `matches!`: constructors alone do not isolate callers if they still depend on
  variants when reading values. Prefer methods that express the caller's intent.
- Variant definition
  - `struct` variant is prohibited. replace with tuple struct variant.  
    Do NOT!

    ```rust
    enum SomeWhat {
        NameAndVersion { name: String, version: String }
    }
    ```

    prefer

    ```rust
    struct NameAndVersion{ name: String, version: String }
    enum SomeWhat {
        NameAndVersion(NameAndVersion)
    }
    ```

    - Rationale: struct variants tend to make
      **consumer-side `match` blocks messy**.  
      They expose field names and layout at every call site, encourage ad-hoc
      deconstruction across the codebase, and turn minor field changes into
      widespread, breaking edits. Using a newtype variant keeps the enum concise
      and centralizes payload behavior on a dedicated struct. It simplifies
      dispatch inside the enum's implementation; it is not a reason to expose
      variant matching to callers. Use constructors and methods at the call
      site.

      Messy with a struct variant:

        ```rust
        enum SomeWhat {
            NameAndVersion { name: String, version: String },
        }

        match v {
            SomeWhat::NameAndVersion { name, version } => {
                // every site repeats field names / layout
            }
        }
        ```

      Keep construction and variant dispatch inside the type:

        ```rust
        struct NameAndVersion { name: String, version: String }

        impl NameAndVersion {
            pub fn name(&self) -> &str { &self.name }
            pub fn version(&self) -> &str { &self.version }
        }

        enum SomeWhat {
            NameAndVersion(NameAndVersion),
        }

        impl SomeWhat {
            pub fn name_and_version(name: &str, version: &str) -> Self {
                Self::NameAndVersion(NameAndVersion {
                    name: name.to_owned(),
                    version: version.to_owned(),
                })
            }

            pub fn name(&self) -> &str {
                match self {
                    Self::NameAndVersion(value) => value.name(),
                }
            }

            pub fn version(&self) -> &str {
                match self {
                    Self::NameAndVersion(value) => value.version(),
                }
            }
        }

        // Callers depend on the interface, not the variants or payload layout.
        let value = SomeWhat::name_and_version("example", "1.0");
        let _ = (value.name(), value.version());
        ```

- Error enums
  - SHOULD take parameters by reference.  
    If ownership is required, perform `copy`/`clone` inside the constructor and
    store it in `self`.
  - **Provide per-variant constructors**.
    - Prefer helpers that return `Result<T, Self>` for guard-style usage.
    - Prefer `#[track_caller]` to preserve call sites in diagnostics.
    - If the condition indicates a bug, **panic in `#[cfg(test)]`** to surface
      it early.

    prefer

    ```rust
    impl SomeError {
        #[track_caller]
        pub fn required_somewhat_not_found<T>(message: &str)
          -> Result<T, Self> 
        {
            #[cfg(test)]
            panic!("bug found! {}", message);

            #[cfg(not(test))]
            Err(Self::RequiredSomewhatNotFound(message.to_string()))
        }
    }

    ```

## Syntax and AST conversion

Keep syntax-to-semantic conversion code declarative at its entry points. A
reader should be able to see which source shape becomes which semantic shape
without tracing parsing details through the dispatch site.

- Use `From` for infallible one-to-one conversions and `TryFrom` for fallible
  one-to-one conversions. Put the conversion implementation on the semantic
  type being constructed.
- Preserve the diagnostic guarantees supported by each parser. PostgreSQL
  conversions use `TryFromWithTokens` when `pg_query` supplies an AST location
  anchor: combine that anchor with the scanned token stream to retain reliable
  source locations in unresolved-AST diagnostics. `turso_parser`'s SQLite AST
  does not carry equivalent node locations, so SQLite conversions MUST NOT
  guess node-level spans from tokens alone. The dialect asymmetry is
  intentional: SQLite diagnostics have lower source-location fidelity rather
  than potentially incorrect locations.
- A `match` over a syntax enum MUST dispatch only. An arm may directly compose
  a payload conversion with the destination's variant constructor, such as
  `Self::null_test(NullTest::try_from(value)?)`, but MUST NOT construct the
  payload's fields or contain conversion-specific logic. Extract that work into
  an individual `From`/`TryFrom` implementation, or into a named type-local
  operation when the source types cannot distinguish the semantic result.
- When a conversion first constructs a semantic payload and then wraps it in
  an enum variant, perform those as separate steps: convert the payload on its
  own type, then call the destination enum's per-variant constructor. The
  enum constructor owns boxing and variant representation details.
- Do not construct another semantic AST type's fields directly from an
  `Expr`/query conversion merely because that type is a variant payload. Move
  the syntax conversion to the payload type instead. For example, convert a
  syntax null test into `NullTest`, then construct `Expr::null_test(...)`.
- Use collection and optional conversion helpers only for their mechanical
  cases. `FromList`/`TryFromList` may collect element-wise `From`/`TryFrom`
  conversions; `FromOpt`/`TryFromOpt` may delegate optional values to the
  corresponding single-value conversion. Keep source-specific collection
  structure, ordering rules, context, or error enrichment in a type-local
  function rather than extending a generic helper.
- Do not hide a conversion rule in a `map` closure at the call site. Prefer a
  conversion trait, a `*Opt`/`*List` helper, or a named local conversion
  function. A closure is acceptable only when it performs local control flow
  rather than translating one domain type into another.

For example, keep the enum dispatch shallow and make the payload and variant
boundaries explicit:

```rust
impl TryFrom<&syn::Expression> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Expression) -> Result<Self, Self::Error> {
        match value {
            syn::Expression::NullTest(value) => {
                Ok(Self::null_test(NullTest::try_from(value)?))
            }
            syn::Expression::Cast(value) => Ok(Self::cast(Cast::try_from(value)?)),
        }
    }
}

impl TryFrom<&syn::NullTest> for NullTest {
    type Error = AnalysisError;

    fn try_from(value: &syn::NullTest) -> Result<Self, Self::Error> {
        // construct NullTest here
    }
}

```

## Tuple

- MUST use tuple struct in public API, anonymous tuple can be use in short and
  limited scope. Do NOT!

  ```rust
  pub fn some_what_pair() -> (String, u32)
  ```

  prefer

  ```rust
  pub struct SomeWhatPair(String, u32);
  pub fn some_what_pair() -> SomeWhatPair;
  ```

## Function implementation

- Use explicit `if`/`else` instead of `bool::then` or `bool::then_some` for
  readability. These methods are disallowed by the repository's Clippy
  configuration.
- Prefer using `let ... else` for early exits.
- Prefer `if let` for single-pattern conditional execution.
- Treat **calling `.iter()` on a foreign type at the call site** as a code smell
  (inside the project).
  - Expose `.iter()` only for *external* consumers of the crate; *internal* code
    should prefer methods on the owning type that encapsulate iteration.
  - Provide a collection newtype and move traversal/transform logic onto it to
    avoid leaking `.iter()` from call sites.

    Do NOT:

    ```rust
    fn do_something(paths: Vec<Path>) {
        // BAD: accessing `iter` on a foreign type here
        if paths.iter().all(|p| p.exists()) {
            // perform something
        }
    }
    ```

    prefer

    ```rust
    pub struct PathCollection(Vec<Path>);

    impl PathCollection {
        pub fn exist_all(&self) -> bool {
            self.0.iter().all( |p| p.exist() ) // access iter on self
        }
    }

    fn do_something( paths: PathCollection ) {
        if paths.exist_all() { // call-site does not touch `.iter()`
            // perform something
        }
    }
    ```

### Express intent above low-level mechanisms

- Write processing and coordination logic in terms of the operations it
  intends to perform. Readers should not need to reconstruct those intentions
  from sequences of low-level primitive operations.
- When channels, synchronization primitives, task handles, or similar
  mechanisms implement a domain protocol, encapsulate that protocol in an
  owning type. Prefer private fields and methods named after the intended
  operations over exposing primitive handles or their operations.
- Encapsulate the complete operation, including acquisition, ownership
  transfer, release, closure, and completion handling where applicable.
  A wrapper is insufficient if callers must still know and perform the
  underlying protocol in the correct order.
- Keep lifetime and failure guarantees with the owning type. Briefly document
  guarantees that affect callers, such as backpressure, cancellation, draining,
  and completion.
- Renaming a general-purpose operation to express domain-specific intent is
  a valid and encouraged abstraction, even when the method only forwards a
  single call. Its value is the meaning it communicates to readers.
- Do not require additional logic to justify an intent-revealing method.
  Evaluate wrappers by the clarity they provide to callers, not by their
  implementation size or complexity. Whether the compiler inlines or eliminates
  a method does not determine its readability value.
- Direct primitive use is appropriate inside these implementations and for
  simple local operations whose purpose is immediately apparent.

Review the callers as well as the wrapper:

- Can a reader understand what the processing does without knowing the
  underlying primitive's API?
- Does the caller express the desired operation, or manually assemble the
  steps required to achieve it?
- Can the primitive or protocol change without rewriting the processing logic?

### Keep method chains readable

- Use method chains for value navigation and transformation, not to obscure
  control flow or error handling.
- Introduce named intermediate values when a chain requires readers to
  mentally reconstruct types or execution paths. Choose names that make each
  value's role apparent.
- Prefer explicit guards and named operations when a chain combines
  validation, resource acquisition, side effects, or error handling in a way
  that hides the processing steps.
- Evaluate readability rather than imposing a fixed limit on chain length.
  A short chain can hide intent, while a longer sequence of straightforward
  transformations can remain clear.
