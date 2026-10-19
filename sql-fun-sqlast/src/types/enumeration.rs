//! A named enumeration preserving its ordered labels.

use crate::sem::FullName;
use crate::types::{TypeDefinition, TypeReference};

use std::any::Any;

/// A named enumeration preserving its ordered labels.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EnumType {
    name: FullName,
    labels: Vec<String>,
}

impl EnumType {
    /// Create an enumeration with labels in declaration order.
    #[must_use]
    pub fn new(name: &FullName, labels: &[String]) -> Self {
        Self {
            name: name.clone(),
            labels: labels.to_vec(),
        }
    }

    /// The resolved qualified name.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// Labels in declaration order.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}

impl TypeDefinition for EnumType {
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
mod tests {
    use super::EnumType;
    use crate::sem::FullName;
    use crate::types::TypeDefinition;
    use crate::types::base::BaseType;

    #[rstest::rstest]
    #[case::same("state", vec!["new".to_owned(), "done".to_owned()], true)]
    #[case::name("other", vec!["new".to_owned(), "done".to_owned()], false)]
    #[case::order("state", vec!["done".to_owned(), "new".to_owned()], false)]
    #[case::label("state", vec!["new".to_owned(), "closed".to_owned()], false)]
    #[case::length("state", vec!["new".to_owned()], false)]
    #[case::empty("state", vec![], false)]
    fn identity_preserves_name_and_ordered_labels(
        #[case] other_name: &str,
        #[case] other_labels: Vec<String>,
        #[case] expected: bool,
    ) {
        let name = FullName::with_schema("public", "state");
        let labels = vec!["new".to_owned(), "done".to_owned()];
        let enumeration = EnumType::new(&name, &labels);
        let other = EnumType::new(&FullName::with_schema("public", other_name), &other_labels);
        assert_eq!(enumeration.name(), &name);
        assert_eq!(enumeration.labels(), labels);
        assert_eq!(enumeration.same_type(&other), expected);
        assert!(enumeration.same_type(enumeration.clone_box().as_ref()));
        assert!(!enumeration.same_type(&BaseType::new(&name)));
    }
}
