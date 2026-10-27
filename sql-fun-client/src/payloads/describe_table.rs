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

impl super::CollectionTraversal for ColumnDefinition {}

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

impl super::CollectionTraversal for TableConstraint {}

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

impl super::CollectionTraversal for DescribeTableResponse {
    fn collections(&self) -> Vec<&dyn super::CollectionReference> {
        vec![&self.columns, &self.constraints]
    }
    fn collections_mut(&mut self) -> Vec<&mut dyn super::CollectionReference> {
        vec![&mut self.columns, &mut self.constraints]
    }
}
