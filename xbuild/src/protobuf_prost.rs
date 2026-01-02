mod enum_type;
mod enum_type_value;
mod message_type;
mod message_type_field;

use std::path::PathBuf;

use crate::Args;
use askama::Template;

use self::{
    enum_type::EnumType, enum_type_value::EnumTypeValue, message_type::MessageType,
    message_type_field::MessageTypeField,
};

#[derive(clap::Parser)]
pub struct EnumTypesCommand {
    source: PathBuf,
}

#[derive(Template)]
#[template(path = "value_enum_template.j2", escape = "none")]
struct ValueEnumTemplate {
    enum_module_name: String,
    enum_type_name: String,
    internal_enum_type_name: String,
    internal_enum_field_name: String,
    fields: Vec<MessageTypeField>,
}

#[derive(Template)]
#[template(path = "kind_enum_template.j2", escape = "none")]
#[expect(clippy::struct_field_names)]
struct KindEnumTemplate {
    enum_module_name: String,
    enum_type_name: String,
    enum_type: EnumType,
}

impl EnumTypesCommand {
    pub fn run(&self, _args: &Args) -> Result<(), anyhow::Error> {
        let schema = std::fs::read_to_string(&self.source)?;
        let message_types = j9::run(".file[].messageType[]", &schema)?;

        for message_type in message_types {
            if j9::run(
                ".field[] | select(.oneofIndex == 0) | length > 0",
                &message_type,
            )?
            .contains(&String::from("true"))
            {
                let enum_type: MessageType = serde_json::from_str(&message_type)?;

                let enum_module_name = enum_type.module_name();
                let enum_type_name = enum_type.enum_type_name();
                let internal_enum_type_name = enum_type.internal_enum_type_name(0_usize);
                let internal_enum_field_name = enum_type.internal_enum_field_name(0_usize);

                let template = ValueEnumTemplate {
                    fields: enum_type.field().clone(),
                    enum_module_name,
                    enum_type_name,
                    internal_enum_type_name,
                    internal_enum_field_name,
                };
                let enum_source = template.render()?;

                println!("{enum_source}");
            }
        }

        let enum_types = j9::run(".file[].enumType[]", &schema)?;

        for enum_type in enum_types {
            let enum_type: EnumType = serde_json::from_str(&enum_type)?;
            let enum_module_name = enum_type.module_name().clone();
            let enum_type_name = enum_type.enum_type_name().clone();
            let template = KindEnumTemplate {
                enum_module_name,
                enum_type_name,
                enum_type,
            };
            let enum_source = template.render()?;

            println!("{enum_source}");
        }

        Ok(())
    }
}

#[derive(Template)]
#[template(path = "struct_template.j2", escape = "none")]
struct StructTemplate {
    module_name: String,
    struct_name: String,
    struct_type: MessageType,
}

#[derive(clap::Parser)]
pub struct StructTypesCommand {
    source: PathBuf,
}

impl StructTypesCommand {
    pub fn run(&self, _args: &Args) -> Result<(), anyhow::Error> {
        let schema = std::fs::read_to_string(&self.source)?;
        let message_types = j9::run(".file[].messageType[]", &schema)?;
        for message_type in message_types {
            let struct_type: MessageType = serde_json::from_str(&message_type)?;
            if struct_type.field().iter().all(|f| f.oneof_index.is_none()) {
                eprintln!("{struct_type:?}");

                let mut struct_source = String::new();

                let struct_name = struct_type.struct_type_name();
                let module_name = struct_type.module_name();

                let getter_template = StructTemplate {
                    module_name,
                    struct_name,
                    struct_type,
                };
                let source = getter_template.render()?;
                struct_source.push_str(&source);

                println!("{struct_source}");
            }
        }
        Ok(())
    }
}
