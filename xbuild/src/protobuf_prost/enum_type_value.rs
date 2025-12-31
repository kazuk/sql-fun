use convert_case::{Case, Casing};

#[derive(serde::Deserialize, serde::Serialize)]
pub struct EnumTypeValue {
    name: String,
    pub(crate) number: i32,
}

impl EnumTypeValue {
    pub fn variant_name(&self, enum_type_name: &str) -> String {
        let pascal_cased = self.name.to_case(Case::Pascal);
        let variant_name = pascal_cased
            .strip_prefix(&enum_type_name)
            .map(str::to_string)
            .unwrap_or(pascal_cased);
        if variant_name.is_empty() {
            String::from(enum_type_name)
        } else {
            variant_name
        }
    }

    pub fn variant_value(&self) -> i32 {
        self.number
    }
}
