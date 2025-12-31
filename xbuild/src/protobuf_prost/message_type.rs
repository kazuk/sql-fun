use convert_case::{Case, Casing};

use super::MessageTypeField;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageType {
    name: String,
    #[serde(default)]
    field: Vec<MessageTypeField>,
    #[serde(default)]
    oneof_decl: Vec<MessageTypeOneofDecl>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
struct MessageTypeOneofDecl {
    name: String,
}

impl MessageType {
    /// module name for MessageType
    pub fn module_name(&self) -> String {
        self.name.to_case(Case::Snake)
    }

    /// enum type name for MessageType
    pub fn enum_type_name(&self) -> String {
        self.name.to_case(Case::Pascal)
    }

    pub fn internal_enum_type_name(&self, oneof_index: usize) -> String {
        let oneof_decl = self.oneof_decl.get(oneof_index).unwrap();
        oneof_decl.name.to_case(Case::Pascal)
    }

    pub fn internal_enum_field_name(&self, oneof_index: usize) -> String {
        let oneof_decl = self.oneof_decl.get(oneof_index).unwrap();
        oneof_decl.name.to_case(Case::Camel)
    }

    /// struct type name for MessageType
    pub fn struct_type_name(&self) -> String {
        self.name.to_case(Case::Pascal)
    }

    pub fn value_param_name(&self) -> String {
        String::from(if self.field.is_empty() {
            "_value"
        } else {
            "value"
        })
    }

    pub fn field(&self) -> &Vec<MessageTypeField> {
        &self.field
    }
}
