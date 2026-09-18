use convert_case::{Case, Casing};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageTypeField {
    name: String,
    number: i32,
    label: String,
    r#type: String,
    json_name: String,
    type_name: Option<String>,
    pub(crate) oneof_index: Option<i32>,
}

impl MessageTypeField {
    /// `field_name`
    pub fn field_name(&self) -> &str {
        &self.name
    }
    /// enum variant name
    pub fn enum_variant_name(&self) -> String {
        self.name.to_case(Case::Pascal)
    }

    pub fn variant_as_fn_name(&self) -> String {
        format!("as_{}", self.name.to_case(Case::Snake))
    }

    pub fn variant_is_fn_name(&self) -> String {
        format!("is_{}", self.name.to_case(Case::Snake))
    }

    pub fn getter_fn_name(&self) -> String {
        format!("get_{}", self.name.to_case(Case::Snake))
    }

    /// enum variant struct name
    pub fn enum_variant_struct_name(&self) -> String {
        if self.r#type == "TYPE_MESSAGE" {
            let type_name_last = self
                .type_name
                .as_ref()
                .expect("must have type_name")
                .split('.')
                .next_back()
                .expect("type_name last");
            type_name_last.to_case(Case::Pascal)
        } else {
            match self.r#type.as_str() {
                "TYPE_INT32" => "i32",
                "TYPE_INT64" => "i64",
                "TYPE_BOOL" => "bool",
                "TYPE_DOUBLE" => "f64",
                "TYPE_STRING" => "std::string::String",
                "TYPE_UINT32" => "u32",
                "TYPE_UINT64" => "u64",
                _ => panic!("unknown type"),
            }
            .to_string()
        }
    }

    /// struct field name
    pub fn struct_field_name(&self) -> String {
        if self.name == "override" || self.name == "type" {
            format!("r#{}", self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn getter_return_type(&self, no_optional: bool) -> String {
        let Some(type_name) = self.type_name.as_ref() else {
            let value_type = match self.r#type.as_str() {
                "TYPE_INT32" => "i32",
                "TYPE_INT64" => "i64",
                "TYPE_BOOL" => "bool",
                "TYPE_DOUBLE" => "f64",
                "TYPE_STRING" => "std::string::String",
                "TYPE_UINT32" => "u32",
                "TYPE_UINT64" => "u64",
                _ => panic!("unknown type"),
            }
            .to_string();
            return if self.label == "LABEL_REPEATED" {
                format!("Vec<{value_type}>")
            } else {
                value_type
            };
        };

        // TYPE_ENUM / TYPE_MESSAGE
        let mut type_name_last = type_name
            .split('.')
            .next_back()
            .unwrap_or(type_name)
            .to_string();
        type_name_last = type_name_last.to_case(Case::Pascal);
        if &type_name_last == "String" {
            type_name_last = String::from("super::String");
        }
        if self.label == "LABEL_REPEATED" {
            format!("{type_name_last}ListOpt")
        } else if no_optional {
            type_name_last
        } else {
            format!("{type_name_last}Opt")
        }
    }

    /// struct field type
    pub fn struct_field_type(&self, no_optional: bool) -> String {
        let Some(type_name) = self.type_name.as_ref() else {
            let value_type = match self.r#type.as_str() {
                "TYPE_INT32" => "i32",
                "TYPE_INT64" => "i64",
                "TYPE_BOOL" => "bool",
                "TYPE_DOUBLE" => "f64",
                "TYPE_STRING" => "std::string::String",
                "TYPE_UINT32" => "u32",
                "TYPE_UINT64" => "u64",
                _ => panic!("unknown type"),
            }
            .to_string();
            return if self.label == "LABEL_REPEATED" {
                format!("Vec<{value_type}>")
            } else {
                value_type
            };
        };

        // TYPE_ENUM / TYPE_MESSAGE
        let mut type_name_last = type_name
            .split('.')
            .next_back()
            .unwrap_or(type_name)
            .to_string();
        type_name_last = type_name_last.to_case(Case::Pascal);
        if &type_name_last == "String" {
            type_name_last = String::from("super::String");
        }
        if self.label == "LABEL_REPEATED" {
            format!("{type_name_last}List")
        } else if no_optional {
            type_name_last
        } else {
            format!("{type_name_last}Opt")
        }
    }

    pub fn struct_field_no_from(&self) -> bool {
        let type_turbo = self.struct_field_type_turbo(false);
        type_turbo == "i32"
            || type_turbo == "i64"
            || type_turbo == "bool"
            || type_turbo == "f32"
            || type_turbo == "f64"
            || type_turbo == "u32"
            || type_turbo == "u64"
            || type_turbo == "Vec::<u64>"
    }

    /// struct field type
    pub fn struct_field_type_turbo(&self, no_optional: bool) -> String {
        let Some(type_name) = self.type_name.as_ref() else {
            let value_type = match self.r#type.as_str() {
                "TYPE_INT32" => "i32",
                "TYPE_INT64" => "i64",
                "TYPE_BOOL" => "bool",
                "TYPE_DOUBLE" => "f64",
                "TYPE_STRING" => "std::string::String",
                "TYPE_UINT32" => "u32",
                "TYPE_UINT64" => "u64",
                _ => panic!("unknown type"),
            }
            .to_string();
            return if self.label == "LABEL_REPEATED" {
                format!("Vec::<{value_type}>")
            } else {
                value_type
            };
        };

        // TYPE_ENUM / TYPE_MESSAGE
        let mut type_name_last = type_name
            .split('.')
            .next_back()
            .unwrap_or(type_name)
            .to_string();
        type_name_last = type_name_last.to_case(Case::Pascal);
        if type_name_last == "String" {
            type_name_last = String::from("super::String");
        }
        if self.label == "LABEL_REPEATED" {
            format!("{type_name_last}List")
        } else if no_optional {
            type_name_last.clone()
        } else {
            format!("{type_name_last}Opt")
        }
    }

    pub const fn struct_field_is_primitive(&self) -> bool {
        self.type_name.is_none()
    }

    pub fn local_var_name(&self) -> String {
        let field_name = self.struct_field_name();
        if field_name == "value" {
            "value_".to_string()
        } else {
            field_name
        }
    }
}
