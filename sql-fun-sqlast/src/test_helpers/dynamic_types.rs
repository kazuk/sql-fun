//! Small owned anonymous-record fixtures; no catalog initialization.
use crate::sem::{
    CompositeFieldCollection, CompositeFieldDefinition, CreateCompositeType, TypeDefinition,
    TypeReference,
};

pub(crate) fn record(field_type: &TypeReference) -> TypeReference {
    let mut fields = CompositeFieldCollection::default();
    fields.push(CompositeFieldDefinition::new(
        "n",
        &Some(field_type.clone()),
        &None,
    ));
    TypeReference::dynamic(&TypeDefinition::from(&CreateCompositeType::from_fields(
        &fields,
    )))
}
