#![cfg(test)]
//! test helper types

mod syn_builder;
mod test_parse_context;

use std::path::PathBuf;

use crate::{
    StringSpan,
    sem::{
        ColumnName, FullName, FunctionParam, OverloadVariant, ScalarConstExpr, SemScalarExpr,
        SourcePathWithSpan, TableName, TypeReference,
    },
};

pub use self::{
    syn_builder::SynBuilder,
    test_parse_context::{TestParseContext, test_context},
};

/// setup default function overload for test
pub fn setup_function_overload() -> (TypeReference, OverloadVariant) {
    let int4 = TypeReference::from_master_type_name("int4");
    let output_param =
        FunctionParam::new_output_param(&Some(String::from("val")), &Some(int4.clone()));
    let overload = OverloadVariant::new(&None, &vec![output_param], false, false);
    (int4, overload)
}

/// default SourcePathWithSpan
#[rstest::fixture]
pub fn source_span() -> SourcePathWithSpan {
    let source_one = PathBuf::from("source_one.sql");
    SourcePathWithSpan::new(&source_one, &StringSpan::default())
}

/// semantic value builder for unit tests
pub struct SemValueBuilder {}

/// new semantic value builder
#[rstest::fixture]
pub fn sem_builder() -> SemValueBuilder {
    SemValueBuilder {}
}

impl SemValueBuilder {
    /// create TableName value
    pub fn table_name(&self, local_name: &str) -> TableName {
        TableName::from(FullName::from_local_name(local_name))
    }

    /// create column_name value
    pub fn column_name(&self, column_name: &str) -> ColumnName {
        ColumnName::from(column_name)
    }

    /// create constaint int expression
    pub fn const_int_expr(&self, value: i32) -> SemScalarExpr {
        SemScalarExpr::Const(ScalarConstExpr::Integer(value))
    }
}
