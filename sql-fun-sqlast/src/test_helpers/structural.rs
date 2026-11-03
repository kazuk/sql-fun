use crate::sem::TypeReference;

pub(crate) fn record(field_type: &TypeReference) -> TypeReference {
    use crate::{
        names::ColumnName,
        sem::{ColumnCollection, ColumnDefinition, TypeDefinition},
    };
    TypeReference::dynamic(&TypeDefinition::ImplicitComposite(ColumnCollection::new(
        vec![ColumnDefinition::new(
            &Some(ColumnName::from("n")),
            Some(field_type),
            None,
            &[],
        )],
    )))
}
