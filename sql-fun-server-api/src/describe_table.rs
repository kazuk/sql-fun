use std::path::PathBuf;

use crate::{Collection, Nullability, TextSpan, TypeReference};

/// column definition
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ColumnDefinition {
    /// column name
    name: String,
    /// column type
    column_type: TypeReference,
    /// nullability
    nullability: Nullability,
    /// column contraints
    constraint: Vec<ColumnConstraint>,
}

/// constraint for column
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ColumnConstraint {
    // TODO: definition
}

/// constraint for column
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TableConstraint {
    // TODO: definition
}

/// argument for [`SqlFunServerApi::describe_table`]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct DescribeTableArgs {
    schema_name: String,
    local_name: String,
}

/// response type for [`SqlFunServerApi::describe_table`]
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct DescribeTableResponse {
    definition_file: PathBuf,
    definition_span: TextSpan,

    columns: Collection<ColumnDefinition>,
    constraints: Collection<TableConstraint>,
}
