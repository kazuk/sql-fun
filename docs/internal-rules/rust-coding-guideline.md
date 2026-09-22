# Rust coding guidelines for `sql-fun`

## Principle: Minimal Work, Maximal Maintainability

This coding guideline exists **to minimize developer effort while keeping code clean**.
It is not a collection of idealistic rules that inflate implementation cost.
Prefer the solution that yields **the fewest moving parts** and **least opportunity for dirt** over time.

### What this implies

- **Public surface = smallest necessary.**  
  Expose only types and functions; no need to access fields/variants and internal shapes.
- **Prefer simple patterns over ceremony.**  
  If a rule increases boilerplate without clear safety or clarity gains, don’t apply it.
- **Pay once, benefit always.**  
  Choose designs that reduce future edits (single source of truth, stable signatures).
- **Opt for standard derivations and conventions.**  
  Use `#[derive(...)]`, `From/TryFrom`, and newtype patterns to avoid ad-hoc glue code.
- **Localize decisions.**  
  Keep logic with the type/enum it belongs to (methods/impls over external `match`es).
- **Stable callers, flexible innards.**  
  Maintain constructor/ctor-like function signatures; feel free to change internals.
- **No premature abstraction.**  
  Don’t add traits/layers unless they demonstrably reduce call-site complexity or churn.

### Review rubric

- Does this change **reduce** future edit points (fewer files/places to touch)?
- Does it **shorten** call-site code or make it more self-documenting?
- Does any extra boilerplate deliver tangible safety/clarity? If not, remove it.
- Could the same effect be achieved with a **private field + ctor + method** instead of exposing structure?
- Will this remain easy to refactor without breaking public signatures?

> Aim for the **lowest hand-count** implementation that keeps the codebase hard to dirty and easy to change.


## Unimplemented functionality

- `todo!()` is Rust's standard-library macro for explicitly marking unimplemented functionality. It is the minimal, correct expression of that state, not a design defect to clean up.
- **Do not replace `todo!()` merely to express "not implemented" in another way.** Adding a dedicated error type, wrapper, abstraction, or fallback solely for that purpose is wasted work: it adds code to maintain without implementing any functionality.
- Preserve existing `todo!()` calls and their behavior during refactoring unless implementing the missing functionality or changing that behavior is explicitly within the task's scope. Replacing the marker is not progress toward implementing the feature.

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

### Test placement

- Place tests **immediately after the implementation block or function they cover**.  
  Co-locating tests with the code they verify reduces the risk of tests being orphaned or forgotten when modules are split or reorganized. Keep the test module focused on the nearby implementation; broader integration tests belong in dedicated test modules/files.
- Test code SHOULD stay at roughly the same abstraction level as the behavior under test.  
  If a unit test needs repeated low-level setup such as writing fixture files, assembling TOML strings, or enumerating environment variables, move that setup behind domain-oriented helpers in a `test_helpers` module.
- Repeated setup in tests MUST be expressed in terms of the test's intent, not the storage or transport details.  
  Prefer helpers such as `write_metadata_project(...)` or `TestMetadataEnvs` over open-coded `write_file`, raw `HashMap` assembly, or large inline fixture literals at each call site.
- When a test reads like filesystem choreography or key-value plumbing instead of a specification of behavior, treat that as a design smell in the test code.  
  Raise the abstraction in `test_helpers` until the test body primarily communicates the scenario, action, and assertion.

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
        let fixture = write_metadata_project_with_cte(&project_dir, "fixture", "qa", "cte")
            .expect("metadata fixture");
        let envs = TestMetadataEnvs::new()
            .with_metadata_project_with_cte(&fixture, "qa")
            .with_sql_dialect("postgresql")
            .into_map();

        // test body now describes behavior instead of setup mechanics
    }
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
  except in cases where it is clearly beneficial for users to see separate modules —  
  for example, when defining distinct concepts such as `syn` / `sem` that would otherwise conflict.


## Struct definitions

- Public fields in `struct` are prohibited.  
  All access must go through constructors, getters, or methods to maintain encapsulation and allow 
  future changes without breaking the public API.
- Constructors
  - For single-parameter cases, 
    it is recommended to implement `From` or `TryFrom` instead of providing a constructor.
  - SHOULD take parameters by reference.  
    If ownership is required, perform `copy`/`clone` inside the constructor and store it in `self`.

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
    If the struct already provides higher-level operations that combine reading and updating, 
    exposing a separate getter may be unnecessary.
- Setter method
  - Prefix with `set_`
  - SHOULD take parameters by reference.
    If ownership is required, perform `copy`/`clone` inside the setter and store it in `self`.
  - Always consider **whether a setter is truly necessary**.  
    If the struct already provides domain-specific operations that cover the update, 
    exposing a generic setter should be avoided.

## Enum definitions

- **Provide per-variant constructors**, and use them to create values outside the type's implementation
  instead of constructing variants directly. Keep constructor signatures focused on semantic inputs,
  rather than requiring callers to construct internal payload types.
  - The purpose is to keep callers independent of the internal representation, not merely to
    standardize construction syntax. For example, changing `enum SomeWhat` into
    `struct SomeWhat(SomeWhatPayload)` should not require rewriting callers if its constructors
    and methods retain the same interface.
- Having a `match` on an `enum` outside of the `enum` itself is a **code smell**.  
  Whenever possible, logic depending on enum variants should be encapsulated within methods
  or trait implementations defined for the enum. This also applies to variant destructuring with
  `if let`, `let ... else`, and `matches!`: constructors alone do not isolate callers if they still
  depend on variants when reading values. Prefer methods that express the caller's intent.
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
    - Rationale: struct variants tend to make **consumer-side `match` blocks messy**.  
        They expose field names and layout at every call site, encourage ad-hoc
        deconstruction across the codebase, and turn minor field changes into
        widespread, breaking edits. Using a newtype variant keeps the enum concise
        and centralizes payload behavior on a dedicated struct. It simplifies dispatch
        inside the enum's implementation; it is not a reason to expose variant matching
        to callers. Use constructors and methods at the call site.

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
    If ownership is required, perform `copy`/`clone` inside the constructor and store it in `self`.
  - **Provide per-variant constructors**.
    - Prefer helpers that return `Result<T, Self>` for guard-style usage.
    - Prefer `#[track_caller]` to preserve call sites in diagnostics.
    - If the condition indicates a bug, **panic in `#[cfg(test)]`** to surface it early.

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

## Tuple

- MUST use tuple struct in public API, anonymous tuple can be use in short and limited scope.
  Do NOT!
  ```rust
  pub fn some_what_pair() -> (String, u32)
  ```
  prefer
  ```rust
  pub struct SomeWhatPair(String, u32);
  pub fn some_what_pair() -> SomeWhatPair;
  ```

## Function implementation.

- Use explicit `if`/`else` instead of `bool::then` or `bool::then_some` for readability. These methods are disallowed by the repository's Clippy configuration.
- Prefer using `let ... else` for early exits.
- Prefer `if let` for single-pattern conditional execution.
- Treat **calling `.iter()` on a foreign type at the call site** as a code smell (inside the project).
  - Expose `.iter()` only for *external* consumers of the crate; *internal* code should prefer methods on the owning type that encapsulate iteration.
  - Provide a collection newtype and move traversal/transform logic onto it to avoid leaking `.iter()` from call sites.

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
