//! test helper types

mod syn_builder;
mod test_parse_context;

use crate::sem::{FunctionParam, OverloadVariant, TypeReference};

pub use self::{
    syn_builder::SynBuilder,
    test_parse_context::{TestParseContext, test_context},
};

///
pub fn setup_function_overload() -> (TypeReference, OverloadVariant) {
    let int4 = TypeReference::from_master_type_name("int4");
    let output_param =
        FunctionParam::new_output_param(&Some(String::from("val")), &Some(int4.clone()));
    let overload = OverloadVariant::new(&None, &vec![output_param], false, false);
    (int4, overload)
}
