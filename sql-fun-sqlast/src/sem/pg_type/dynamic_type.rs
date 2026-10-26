use crate::sem::TypeDefinition;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DynamicTypeReference {
    type_def: Box<TypeDefinition>,
}
impl DynamicTypeReference {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        self.type_def.rename_relation(old, new);
    }

    pub fn new(type_def: &TypeDefinition) -> Self {
        Self {
            type_def: Box::new(type_def.clone()),
        }
    }

    pub fn type_definition(&self) -> &TypeDefinition {
        &self.type_def
    }

    pub fn is_array(&self) -> Option<bool> {
        self.type_def.is_array()
    }
}
