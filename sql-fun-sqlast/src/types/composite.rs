//! Owned, structural members of composite and table-row types.

use crate::sem::{ColumnName, FullName};
use crate::types::{TypeDefinition, TypeReference};

use std::any::Any;

/// An ordered record member with optional name and NOT NULL information.
#[derive(Debug, Clone, Eq)]
pub struct Member {
    name: Option<ColumnName>,
    type_: TypeReference,
    is_not_null: Option<bool>,
}

impl PartialEq for Member {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.is_not_null == other.is_not_null
            && self.type_.same_type(other.type_.as_ref())
    }
}

impl Member {
    /// Create an owned member from its semantic components.
    #[must_use]
    pub fn new(
        name: &Option<ColumnName>,
        type_: &dyn TypeDefinition,
        is_not_null: Option<bool>,
    ) -> Self {
        Self {
            name: name.clone(),
            type_: type_.clone_box(),
            is_not_null,
        }
    }

    /// The member name, if known.
    #[must_use]
    pub const fn name(&self) -> Option<&ColumnName> {
        self.name.as_ref()
    }

    /// The owned member's loaded type.
    #[must_use]
    pub fn member_type(&self) -> &dyn TypeDefinition {
        self.type_.as_ref()
    }

    /// Whether the member is declared NOT NULL, if known.
    #[must_use]
    pub const fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }
}

#[cfg(test)]
mod member_tests {
    use super::Member;
    use crate::sem::{ColumnName, FullName};
    use crate::types::base::BaseType;

    #[test]
    fn preserves_member_components_in_identity() {
        let integer = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
        let name = Some(ColumnName::new("value"));
        let member = Member::new(&name, &integer, Some(false));
        assert_eq!(member.name(), name.as_ref());
        assert_eq!(member.is_not_null(), Some(false));
        assert_eq!(member.member_type().as_any().downcast_ref(), Some(&integer));
        assert_eq!(member, member.clone());
        assert_ne!(member, Member::new(&None, &integer, Some(false)));
        assert_ne!(member, Member::new(&name, &text, Some(false)));
        assert_ne!(member, Member::new(&name, &integer, None));
        assert_ne!(member, Member::new(&name, &integer, Some(true)));
    }
}

/// An ordered collection of record members.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Members(Vec<Member>);

impl Members {
    fn contains_polymorphic(&self) -> bool {
        self.0
            .iter()
            .any(|member| member.type_.contains_polymorphic())
    }

    /// Clone the supplied members, retaining their order.
    #[must_use]
    pub fn new(members: &[Member]) -> Self {
        Self(members.to_vec())
    }

    /// Members in declaration order.
    #[must_use]
    pub fn members(&self) -> &[Member] {
        &self.0
    }

    /// Find the first member with the exact logical name.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&Member> {
        self.0
            .iter()
            .find(|member| member.name().is_some_and(|value| value.as_str() == name))
    }
}

#[cfg(test)]
mod members_tests {
    use super::{Member, Members};
    use crate::sem::{ColumnName, FullName};
    use crate::types::base::BaseType;

    #[test]
    fn preserves_order_and_resolves_only_named_members() {
        let integer = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let unnamed = Member::new(&None, &integer, None);
        let named = Member::new(&Some(ColumnName::new("Value")), &integer, Some(true));
        let members = Members::new(&[unnamed.clone(), named.clone()]);
        assert_eq!(members.members(), &[unnamed.clone(), named.clone()]);
        assert_eq!(members.member("Value"), Some(&named));
        assert_eq!(members.member("value"), None);
        assert_eq!(members.member(""), None);
        assert_ne!(members, Members::new(&[named, unnamed]));
    }
}

/// An explicitly declared composite type and its structural members.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CompositeType {
    name: Option<FullName>,
    members: Members,
}

impl CompositeType {
    /// Create a composite, retaining an optional resolved qualified name.
    #[must_use]
    pub fn new(name: &Option<FullName>, members: &Members) -> Self {
        Self {
            name: name.clone(),
            members: members.clone(),
        }
    }

    /// The resolved qualified name, if known.
    #[must_use]
    pub const fn name(&self) -> Option<&FullName> {
        self.name.as_ref()
    }

    /// The ordered structural members.
    #[must_use]
    pub const fn members(&self) -> &Members {
        &self.members
    }
}

impl TypeDefinition for CompositeType {
    fn contains_polymorphic(&self) -> bool {
        self.members.contains_polymorphic()
    }

    fn as_any(&self) -> &dyn Any {
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
mod composite_tests {
    use super::{CompositeType, Members};
    use crate::sem::FullName;
    use crate::types::TypeDefinition;

    #[test]
    fn identity_includes_optional_qualified_name() {
        let members = Members::new(&[]);
        let name = Some(FullName::with_schema("public", "record"));
        let value = CompositeType::new(&name, &members);
        assert_eq!(value.name(), name.as_ref());
        assert_eq!(value.members(), &members);
        assert!(value.same_type(value.clone_box().as_ref()));
        assert!(!value.same_type(&CompositeType::new(&None, &members)));
        assert!(!value.same_type(&CompositeType::new(
            &Some(FullName::with_schema("other", "record")),
            &members,
        )));
    }
}

/// An anonymous composite inferred from ordered members.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImplicitCompositeType {
    members: Members,
}

impl ImplicitCompositeType {
    /// Create an implicit composite with owned members.
    #[must_use]
    pub fn new(members: &Members) -> Self {
        Self {
            members: members.clone(),
        }
    }

    /// The ordered structural members.
    #[must_use]
    pub const fn members(&self) -> &Members {
        &self.members
    }
}

impl TypeDefinition for ImplicitCompositeType {
    fn contains_polymorphic(&self) -> bool {
        self.members.contains_polymorphic()
    }

    fn as_any(&self) -> &dyn Any {
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
mod implicit_composite_tests {
    use super::{CompositeType, ImplicitCompositeType, Member, Members};
    use crate::sem::FullName;
    use crate::types::{TypeDefinition, base::BaseType};

    #[test]
    fn compares_members_without_conflating_explicit_composites() {
        let members = Members::new(&[]);
        let value = ImplicitCompositeType::new(&members);
        assert_eq!(value.members(), &members);
        assert!(value.same_type(value.clone_box().as_ref()));
        assert!(!value.same_type(&CompositeType::new(&None, &members)));
        let integer = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
        let populated = Members::new(&[Member::new(&None, &integer, None)]);
        assert!(!value.same_type(&ImplicitCompositeType::new(&populated)));
    }
}

/// The named row type of a table, with owned structural members.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TableRowType {
    name: FullName,
    members: Members,
}

impl TableRowType {
    /// Create a table row from its resolved table name and members.
    #[must_use]
    pub fn new(name: &FullName, members: &Members) -> Self {
        Self {
            name: name.clone(),
            members: members.clone(),
        }
    }

    /// The resolved qualified table name.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// The ordered structural members.
    #[must_use]
    pub const fn members(&self) -> &Members {
        &self.members
    }
}

impl TypeDefinition for TableRowType {
    fn contains_polymorphic(&self) -> bool {
        self.members.contains_polymorphic()
    }

    fn as_any(&self) -> &dyn Any {
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
mod table_row_tests {
    use super::{CompositeType, ImplicitCompositeType, Members, TableRowType};
    use crate::sem::FullName;
    use crate::types::TypeDefinition;

    #[test]
    fn retains_table_identity_and_distinct_category() {
        let name = FullName::with_schema("public", "items");
        let members = Members::new(&[]);
        let value = TableRowType::new(&name, &members);
        assert_eq!(value.name(), &name);
        assert_eq!(value.members(), &members);
        assert!(value.same_type(value.clone_box().as_ref()));
        assert!(!value.same_type(&TableRowType::new(
            &FullName::with_schema("other", "items"),
            &members,
        )));
        assert!(!value.same_type(&CompositeType::new(&Some(name), &members)));
        assert!(!value.same_type(&ImplicitCompositeType::new(&members)));
    }
}
