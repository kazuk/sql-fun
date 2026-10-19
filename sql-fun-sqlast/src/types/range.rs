//! Owned range structures; catalog loading is a separate adapter concern.

use crate::sem::FullName;
use crate::types::{TypeDefinition, TypeReference};
use std::any::Any;

/// A nominal range identity and its owned subtype, possibly a constraint.
#[derive(Debug, Clone, Eq)]
pub struct RangeType {
    name: FullName,
    subtype: TypeReference,
}
impl RangeType {
    /// Construct a complete range value without catalog validation.
    pub fn new(name: &FullName, subtype: &dyn TypeDefinition) -> Self {
        Self {
            name: name.clone(),
            subtype: subtype.clone_box(),
        }
    }
    /// The range's nominal identity; equal subtypes do not imply equal ranges.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// The owned subtype.
    pub fn subtype(&self) -> &dyn TypeDefinition {
        self.subtype.as_ref()
    }
}
impl PartialEq for RangeType {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.subtype.same_type(other.subtype.as_ref())
    }
}
impl TypeDefinition for RangeType {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }
    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
    fn contains_polymorphic(&self) -> bool {
        self.subtype.contains_polymorphic()
    }
}

/// A nominal multirange and its complete, related range value.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MultirangeType {
    name: FullName,
    range: RangeType,
}
impl MultirangeType {
    /// Construct a multirange retaining both nominal identities and the subtype.
    pub fn new(name: &FullName, range: &RangeType) -> Self {
        Self {
            name: name.clone(),
            range: range.clone(),
        }
    }
    /// The multirange's nominal identity.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// The complete related range.
    pub const fn range(&self) -> &RangeType {
        &self.range
    }
}
impl TypeDefinition for MultirangeType {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }
    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
    fn contains_polymorphic(&self) -> bool {
        self.range.contains_polymorphic()
    }
}

#[cfg(test)]
mod tests {
    use super::{MultirangeType, RangeType};
    use crate::sem::FullName;
    use crate::types::{TypeDefinition, base::BaseType};
    #[test]
    fn range_identity_is_not_just_its_subtype() {
        let subtype = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let first = RangeType::new(&FullName::with_schema("public", "first"), &subtype);
        let second = RangeType::new(&FullName::with_schema("public", "second"), &subtype);
        assert!(!first.same_type(&second));
        assert!(first.same_type(first.clone_box().as_ref()));
        let name = FullName::with_schema("public", "multi");
        let multi = MultirangeType::new(&name, &first);
        assert!(!multi.same_type(&MultirangeType::new(&name, &second)));
        assert!(multi.range().subtype().same_type(&subtype));
        assert!(!multi.contains_polymorphic());
    }
}
