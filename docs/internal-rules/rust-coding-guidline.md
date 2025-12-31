# Rust coding guidlines for `sql-fun`

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


## module sturcture

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

#[cfg(test)]
mod tests {
    // tests goes here
    //   The basic rules also apply under the `tests` module.  
    //   Whether code reviews should strongly enforce them is a separate matter.
}

```

- importing items
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

- Having a `match` on an `enum` outside of the `enum` itself is a **code smell**.  
  Whenever possible, logic depending on enum variants should be encapsulated within methods 
  or trait implementations defined for the enum.
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
        widespread, breaking edits. Using a newtype variant keeps the enum concise,
        centralizes behavior on a dedicated struct, and lets consumers match only on
        the variant **name** (not its internal shape).

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

        Cleaner with a newtype variant:
        ```rust
        struct NameAndVersion { name: String, version: String }

        impl NameAndVersion {
            pub fn name(&self) -> &str { &self.name }
            pub fn version(&self) -> &str { &self.version }
        }

        enum SomeWhat {
            NameAndVersion(NameAndVersion),
        }

        match v {
            SomeWhat::NameAndVersion(nv) => {
                // call methods, internal layout stays encapsulated
                let _ = (nv.name(), nv.version());
            }
        }
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

- MUST use tuple struct in public API, anonymouse tuple can be use in short and limited scope.
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
