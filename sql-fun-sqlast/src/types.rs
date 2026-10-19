//! Opt-in, owned type values independent of legacy semantic storage.
//!
//! Each category defines its identity components; equality compares categories
//! and those components recursively. It does not imply SQL cast compatibility.
//! See [`compat`] for explicit conversion and catalog-dependent loading, and the
//! API audit in `docs/design-docs/loaded-types.md` for migration boundaries.

pub mod array;
pub mod base;
mod cast;
pub mod compat;
mod composite;
mod constraint;
mod domain;
mod enumeration;

pub use self::cast::{CastContext, CastDefinition};
pub use self::composite::{CompositeType, ImplicitCompositeType, Member, Members, TableRowType};
pub use self::constraint::{Any, Candidates, Never};
pub use self::domain::DomainType;
pub use self::enumeration::EnumType;

use std::any::Any as StdAny;

/// An owned type value with category-defined, recursively comparable components.
///
/// Implementations must make `same_type` an equivalence relation, comparing
/// concrete category as well as all identity components. Cloning preserves
/// identity and owns the cloned components independently of catalog storage.
pub trait TypeDefinition: std::fmt::Debug + StdAny {
    /// Inspect a category to access its category-specific capabilities.
    fn as_any(&self) -> &dyn StdAny;
    /// Clone this value and its owned components.
    fn clone_box(&self) -> TypeReference;
    /// Compare category and identity components, without considering coercions.
    fn same_type(&self, other: &dyn TypeDefinition) -> bool;
}

/// An owned, loaded type value, including structural and anonymous types.
pub type TypeReference = Box<dyn TypeDefinition>;

impl Clone for TypeReference {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl PartialEq for dyn TypeDefinition + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.same_type(other)
    }
}

impl Eq for dyn TypeDefinition + '_ {}
