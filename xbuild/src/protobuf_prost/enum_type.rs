use convert_case::{Case, Casing};

use crate::protobuf_prost::EnumTypeValue;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumType {
    name: String,
    #[serde(default)]
    pub(crate) value: Vec<EnumTypeValue>,
}
impl EnumType {
    /// module name for MessageType
    pub fn module_name(&self) -> String {
        self.name.to_case(Case::Snake)
    }
    /// enum type name for MessageType
    pub fn enum_type_name(&self) -> String {
        self.name.to_case(Case::Pascal)
    }
    /// None variant value
    pub fn none_variant_value(&self) -> i32 {
        self.value
            .iter()
            .map(|v| v.number)
            .max()
            .unwrap_or_default()
            + 1
    }
}
