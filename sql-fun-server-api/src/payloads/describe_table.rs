use std::path::PathBuf;

use crate::{
    Collection,
    payloads::{Nullability, TextSpan, TypeReference},
};

/// column definition
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct ColumnDefinition {
    /// column name
    name: String,
    /// column type
    column_type: TypeReference,
    /// nullability
    nullability: Nullability,
    /// column constraints
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

/// argument for `describe_table`
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct DescribeTableArgs {
    schema_name: String,
    local_name: String,
}

/// response type for `describe_table`
#[derive(serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub struct DescribeTableResponse {
    definition_file: PathBuf,
    definition_span: TextSpan,

    columns: Collection<ColumnDefinition>,
    constraints: Collection<TableConstraint>,
}

impl DescribeTableResponse {
    /// Mutably access the columns collection.
    pub const fn columns_mut(&mut self) -> &mut Collection<ColumnDefinition> {
        &mut self.columns
    }
    /// Mutably access the constraints collection.
    pub const fn constraints_mut(&mut self) -> &mut Collection<TableConstraint> {
        &mut self.constraints
    }
}
