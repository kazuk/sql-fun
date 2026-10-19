//! Owned, directed cast relationships, independent of type equality.

mod query;
mod resolve;
pub use self::query::{CastQueryDirection, CastQueryError, CastQueryOutcome, DeferredCastQuery};

pub(super) use self::resolve::CastResolver;

use crate::types::{TypeDefinition, TypeReference};

/// The catalog's eligibility and identity distinction for a direct cast.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CastContext {
    /// An implicit conversion marked preferred by the catalog.
    ImplicitPreferred,
    /// An ordinary implicit conversion.
    Implicit,
    /// A conversion available in assignment contexts.
    Assignment,
    /// A conversion requiring an explicit cast.
    Explicit,
    /// Identity with no conversion.
    NoConversion,
}

impl CastContext {
    /// An implicit conversion marked preferred by the catalog.
    #[must_use]
    pub const fn implicit_preferred() -> Self {
        Self::ImplicitPreferred
    }

    /// An ordinary implicit conversion.
    #[must_use]
    pub const fn implicit() -> Self {
        Self::Implicit
    }

    /// A conversion available in assignment contexts.
    #[must_use]
    pub const fn assignment() -> Self {
        Self::Assignment
    }

    /// A conversion requiring an explicit cast.
    #[must_use]
    pub const fn explicit() -> Self {
        Self::Explicit
    }

    /// Identity with no conversion.
    #[must_use]
    pub const fn no_conversion() -> Self {
        Self::NoConversion
    }

    /// Whether the catalog permits this relationship in implicit contexts.
    #[must_use]
    pub const fn use_in_implicit(&self) -> bool {
        matches!(
            self,
            Self::ImplicitPreferred | Self::Implicit | Self::NoConversion
        )
    }

    /// Whether the catalog permits this relationship in assignment contexts.
    #[must_use]
    pub const fn use_in_assignment(&self) -> bool {
        !matches!(self, Self::Explicit)
    }

    /// Every represented relationship permits an explicit cast.
    #[must_use]
    pub const fn use_in_explicit(&self) -> bool {
        true
    }

    /// Whether this is an implicit conversion marked preferred.
    #[must_use]
    pub const fn is_implicit_preferred(&self) -> bool {
        matches!(self, Self::ImplicitPreferred)
    }

    /// Whether this relationship requires no conversion.
    #[must_use]
    pub const fn is_no_conversion(&self) -> bool {
        matches!(self, Self::NoConversion)
    }
}

#[cfg(test)]
mod context_tests {
    use super::CastContext;

    #[rstest::rstest]
    #[case::preferred(CastContext::implicit_preferred(), true, true, true, false)]
    #[case::implicit(CastContext::implicit(), true, true, false, false)]
    #[case::assignment(CastContext::assignment(), false, true, false, false)]
    #[case::explicit(CastContext::explicit(), false, false, false, false)]
    #[case::identity(CastContext::no_conversion(), true, true, false, true)]
    fn preserves_each_context_distinction(
        #[case] context: CastContext,
        #[case] implicit: bool,
        #[case] assignment: bool,
        #[case] preferred: bool,
        #[case] identity: bool,
    ) {
        assert_eq!(context.use_in_implicit(), implicit);
        assert_eq!(context.use_in_assignment(), assignment);
        assert!(context.use_in_explicit());
        assert_eq!(context.is_implicit_preferred(), preferred);
        assert_eq!(context.is_no_conversion(), identity);
    }
}

/// A direct, ordered relationship between two complete loaded type values.
///
/// Equality compares both endpoints recursively and the cast context. This
/// value does not implement `TypeDefinition`: cast eligibility never establishes
/// equality of the endpoint types.
#[derive(Debug, Clone, Eq)]
pub struct CastDefinition {
    source: TypeReference,
    target: TypeReference,
    context: CastContext,
}

impl PartialEq for CastDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.source.same_type(other.source.as_ref())
            && self.target.same_type(other.target.as_ref())
            && self.context == other.context
    }
}

impl CastDefinition {
    /// Own independent clones of both endpoints and preserve the cast context.
    #[must_use]
    pub fn new(
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
        context: &CastContext,
    ) -> Self {
        Self {
            source: source.clone_box(),
            target: target.clone_box(),
            context: *context,
        }
    }

    /// The loaded source type of this directed relationship.
    #[must_use]
    pub fn source(&self) -> &dyn TypeDefinition {
        self.source.as_ref()
    }

    /// The loaded target type of this directed relationship.
    #[must_use]
    pub fn target(&self) -> &dyn TypeDefinition {
        self.target.as_ref()
    }

    /// The catalog's original context distinction.
    #[must_use]
    pub const fn context(&self) -> CastContext {
        self.context
    }
}

#[cfg(test)]
mod relationship_tests {
    use super::{CastContext, CastDefinition};
    use crate::types::{TypeDefinition, array::ArrayType, base::BaseType};

    #[test]
    fn cloned_relationship_owns_complete_recursive_endpoints() {
        let cast = {
            let source = ArrayType::new(&BaseType::new(
                &"pg_catalog.int2".parse().expect("qualified builtin name"),
            ));
            let target = ArrayType::new(&BaseType::new(
                &"pg_catalog.int4".parse().expect("qualified builtin name"),
            ));
            CastDefinition::new(&source, &target, &CastContext::implicit())
        };
        let cloned = cast.clone();

        assert_eq!(cast, cloned);
        assert!(cloned.source().as_any().is::<ArrayType>());
        assert!(cloned.target().as_any().is::<ArrayType>());
        assert!(!cloned.source().same_type(cloned.target()));
    }

    #[test]
    fn equality_requires_ordered_endpoints_and_context() {
        let source = BaseType::new(&"pg_catalog.int2".parse().expect("qualified builtin name"));
        let target = BaseType::new(&"pg_catalog.int4".parse().expect("qualified builtin name"));
        let cast = CastDefinition::new(&source, &target, &CastContext::implicit());
        let reverse = CastDefinition::new(&target, &source, &CastContext::implicit());
        let explicit = CastDefinition::new(&source, &target, &CastContext::explicit());
        let preferred = CastDefinition::new(&source, &target, &CastContext::implicit_preferred());

        assert_ne!(cast, reverse);
        assert_ne!(cast, explicit);
        assert_ne!(cast, preferred);
        assert!(cast.source().same_type(explicit.source()));
        assert!(cast.target().same_type(explicit.target()));
        assert!(!source.same_type(&target));
    }

    #[test]
    fn identity_is_a_relationship_between_equal_types() {
        let source = BaseType::new(&"pg_catalog.int4".parse().expect("qualified builtin name"));
        let identity = CastDefinition::new(&source, &source, &CastContext::no_conversion());

        assert!(identity.source().same_type(identity.target()));
        assert!(identity.context().is_no_conversion());
    }
}
