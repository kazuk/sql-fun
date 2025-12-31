#![doc = include_str!("syn.md")]

mod generated;
mod syn_ext;

use itertools::Itertools;

pub use self::{
    generated::*,
    syn_ext::{ListOpt, Opt},
};

impl TypeName {
    /// convert TypeName to full_name string
    pub fn full_name(&self) -> std::string::String {
        let Some(names) = self.get_names().as_inner() else {
            return std::string::String::from("");
        };
        names
            .iter()
            .filter_map(|v| v.as_string().get_sval())
            .join(".")
    }
}
