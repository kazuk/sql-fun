//! Adapt legacy resolved spellings only; no catalog access or scope allocation.

use crate::sem::TypeReference as LegacyReference;
use crate::types::{PolymorphicCategory, PolymorphicPosition, PolymorphicRole};

/// Adapt a resolved legacy occurrence for atomic signature construction.
///
/// Feed every returned position to one `SignaturePolymorphism::new` call.
/// Unqualified names are not resolved implicitly. This function only recognizes
/// concrete, scalar `pg_catalog` references and performs no catalog lookup.
pub fn polymorphic_position(
    reference: &LegacyReference,
    role: PolymorphicRole,
) -> Option<PolymorphicPosition> {
    if reference.is_dynamic() || reference.is_array() != Some(false) {
        return None;
    }
    let category = PolymorphicCategory::from_postgres_name(reference.full_name())?;
    Some(PolymorphicPosition::new(category, role))
}
