//! Loaded type constraints independent of catalog lookup and selection policy.

use crate::types::{TypeDefinition, TypeReference};
use std::any::Any as StdAny;

/// An unconstrained type value.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Any;

impl Any {
    /// Create an unconstrained type value.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TypeDefinition for Any {
    fn as_any(&self) -> &dyn StdAny {
        self
    }

    fn clone_box(&self) -> TypeReference {
        Box::new(*self)
    }

    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().is::<Self>()
    }
}

#[cfg(test)]
mod any_tests {
    use super::Any;
    use crate::sem::FullName;
    use crate::types::{TypeDefinition, base::BaseType};

    #[test]
    fn clone_preserves_unconstrained_category() {
        let value = Any::new();
        assert!(value.same_type(value.clone_box().as_ref()));
        assert!(!value.same_type(&super::Never::new()));
    }

    #[rstest::rstest]
    #[case::unknown("unknown")]
    #[case::polymorphic("anyelement")]
    fn named_values_are_distinct_from_unconstrained(#[case] name: &str) {
        // Direct category construction does not claim catalog loading support.
        let named = BaseType::new(&FullName::with_schema("pg_catalog", name));
        assert!(!Any::new().same_type(&named));
        assert!(!named.same_type(&Any::new()));
    }
}

/// A constraint with no possible type value.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Never;

impl Never {
    /// Create a constraint with no possible type value.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TypeDefinition for Never {
    fn as_any(&self) -> &dyn StdAny {
        self
    }

    fn clone_box(&self) -> TypeReference {
        Box::new(*self)
    }

    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().is::<Self>()
    }
}

#[cfg(test)]
mod never_tests {
    use super::Never;
    use crate::sem::FullName;
    use crate::types::{TypeDefinition, base::BaseType};

    #[test]
    fn clone_preserves_empty_constraint_category() {
        let value = Never::new();
        assert!(value.same_type(value.clone_box().as_ref()));
        assert!(!value.same_type(&super::Any::new()));
        let named = BaseType::new(&FullName::with_schema("fixture", "value"));
        assert!(!value.same_type(&named));
    }
}

/// A nonempty set of complete candidate type values.
///
/// Identity ignores order and duplicate candidates. Construction preserves the
/// supplied snapshots, including nested constraints, without choosing a value.
#[derive(Debug, Clone, Eq)]
pub struct Candidates {
    candidates: Vec<TypeReference>,
}

impl Candidates {
    /// Own clones of the candidates, or return [`Never`] for an empty set.
    ///
    /// A singleton remains a candidate constraint. Nested constraints are not
    /// flattened, and no ranking or selection is performed.
    #[must_use]
    pub fn from_candidates(candidates: &[TypeReference]) -> TypeReference {
        if candidates.is_empty() {
            Box::new(Never::new())
        } else {
            Box::new(Self {
                candidates: candidates.to_vec(),
            })
        }
    }

    /// Inspect the complete supplied values, including their original order.
    #[must_use]
    pub fn candidates(&self) -> &[TypeReference] {
        &self.candidates
    }

    fn contains(&self, candidate: &dyn TypeDefinition) -> bool {
        self.candidates
            .iter()
            .any(|value| value.same_type(candidate))
    }
}

impl PartialEq for Candidates {
    fn eq(&self, other: &Self) -> bool {
        self.candidates
            .iter()
            .all(|candidate| other.contains(candidate.as_ref()))
            && other
                .candidates
                .iter()
                .all(|candidate| self.contains(candidate.as_ref()))
    }
}

impl TypeDefinition for Candidates {
    fn as_any(&self) -> &dyn StdAny {
        self
    }

    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }

    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{Any, Candidates, Never};
    use crate::sem::FullName;
    use crate::types::{TypeReference, array::ArrayType, base::BaseType};

    fn named(name: &str) -> TypeReference {
        Box::new(BaseType::new(&FullName::with_schema("fixture", name)))
    }

    #[test]
    fn empty_candidates_produce_never_and_singleton_retains_constraint() {
        let empty = Candidates::from_candidates(&[]);
        assert!(empty.as_any().is::<Never>());
        let value = named("value");
        let singleton = Candidates::from_candidates(std::slice::from_ref(&value));
        assert!(singleton.as_any().is::<Candidates>());
        assert!(!singleton.same_type(value.as_ref()));
    }

    #[test]
    fn identity_ignores_order_and_duplicates_but_preserves_membership() {
        let first = named("first");
        let second = named("second");
        let left = Candidates::from_candidates(&[first.clone(), second.clone()]);
        let right = Candidates::from_candidates(&[second, first.clone(), first.clone()]);
        let subset = Candidates::from_candidates(&[first]);
        assert!(left.same_type(right.as_ref()));
        assert!(right.same_type(left.as_ref()));
        assert!(!left.same_type(subset.as_ref()));
        assert!(!subset.same_type(left.as_ref()));
        assert_eq!(
            right
                .as_any()
                .downcast_ref::<Candidates>()
                .unwrap()
                .candidates()
                .len(),
            3
        );
    }

    #[test]
    fn clone_owns_complete_recursive_candidates_without_flattening() {
        let loaded = {
            let array: TypeReference = Box::new(ArrayType::new(named("element").as_ref()));
            let nested = Candidates::from_candidates(&[array, Box::new(Any::new())]);
            Candidates::from_candidates(&[nested, Box::new(Never::new())])
        };
        let cloned = loaded.clone();
        assert!(loaded.same_type(cloned.as_ref()));
        assert!(cloned.same_type(loaded.as_ref()));
        let outer = cloned.as_any().downcast_ref::<Candidates>().unwrap();
        assert!(outer.candidates()[0].as_any().is::<Candidates>());
        assert!(outer.candidates()[1].as_any().is::<Never>());
        let flattened = Candidates::from_candidates(&[
            Box::new(ArrayType::new(named("element").as_ref())),
            Box::new(Any::new()),
            Box::new(Never::new()),
        ]);
        assert!(!loaded.same_type(flattened.as_ref()));
    }
}
