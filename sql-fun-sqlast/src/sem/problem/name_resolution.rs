mod column_not_found;

use super::AnalysisProblem;

use crate::sem::{
    CteName, FullName, TableName, TypeReference,
    create_table::{ColumnName, ViewName},
    data_source::AliasName,
};

/// Name resolution problems
#[derive(Debug, Clone)]
pub enum NameResolution {
    /// type .1 not found in current context
    ///   on create domain .0
    DomainTypeNameNotFound(FullName, String),

    /// function not found
    FunctionNotFound(FullName),

    /// table function not found
    TableFunctionNotFound(FullName),

    /// column reference not resolved
    UndefinedColumn(String),

    /// type not found in column definition
    ColumnTypeNotFound(TableName, ColumnName, FullName),

    /// column not found in table function
    ColumnNotFoundInTableFunction(AliasName, ColumnName),

    /// column not found in table
    ColumnNotFoundInTable(TableName, ColumnName),

    /// column not found in CTE
    ColumnNotFoundInCte(CteName, ColumnName),

    /// column not found in subquery
    ColumnNotFoundInSubQuery(AliasName, ColumnName),

    /// column not found in source
    ColumnNotFoundInSource(ColumnName),

    /// column not found in view
    ColumnNotFoundInView(ViewName, ColumnName),

    /// column reference ambiguous
    AmbiguousColumn(ColumnName),

    /// same alias defined
    DuplicateAlias(AliasName),

    /// data source not defined in from clause
    UndefinedAlias(String),

    /// table .0 not found in current context
    RelationNotFound(String),

    /// view not found
    ViewNotFound(ViewName),

    /// index not found
    IndexNotFound(String),

    /// sequence not found
    SequenceNotFound(FullName),

    /// table not found
    TableNotFound(TableName),

    /// composite type field not found
    CompositeTypeFieldNotFound(TypeReference, String),
}

impl NameResolution {
    pub fn duplicate_alias(alias_name: &AliasName) -> Self {
        Self::DuplicateAlias(alias_name.clone())
    }

    pub fn composite_type_field_not_found(type_ref: &TypeReference, field_name: &str) -> Self {
        Self::CompositeTypeFieldNotFound(type_ref.clone(), field_name.to_string())
    }

    pub fn ambiguous_column(column_name: &ColumnName) -> Self {
        Self::AmbiguousColumn(column_name.clone())
    }

    pub fn table_not_found(table_name: &TableName) -> Self {
        Self::TableNotFound(table_name.clone())
    }

    pub fn sequence_not_found(sequence_name: &FullName) -> Self {
        Self::SequenceNotFound(sequence_name.clone())
    }

    pub fn index_not_found(index_name: &str) -> Self {
        Self::IndexNotFound(index_name.to_string())
    }

    pub fn view_not_found(view_name: &ViewName) -> Self {
        Self::ViewNotFound(view_name.clone())
    }

    /// domain not found
    #[tracing::instrument()]
    pub fn domain_type_name_not_found(
        domain_name: &FullName,
        type_name: &crate::syn::TypeName,
    ) -> Self {
        Self::DomainTypeNameNotFound(domain_name.clone(), type_name.full_name())
    }

    /// specified table not found
    #[tracing::instrument()]
    pub fn relation_not_found(table_name: &TableName) -> Self {
        Self::RelationNotFound(table_name.to_string())
    }

    /// calling function not found
    pub fn function_not_found(func_name: &FullName) -> Self {
        Self::FunctionNotFound(func_name.clone())
    }

    /// specified table function not found
    pub fn table_function_not_found(call: &FullName) -> Self {
        Self::TableFunctionNotFound(call.clone())
    }

    /// table column type
    #[tracing::instrument()]
    pub fn column_type_not_found(
        table_name: &TableName,
        colname: &ColumnName,
        type_name: &FullName,
    ) -> Self {
        Self::ColumnTypeNotFound(table_name.clone(), colname.clone(), type_name.clone())
    }

    /// alias name undefined
    pub fn undefined_alias(alias_name: &String) -> Self {
        Self::UndefinedAlias(alias_name.to_string())
    }
}

impl AnalysisProblem {
    /// alias name duplicate in `FROM` clause
    #[tracing::instrument()]
    pub fn duplicate_alias(alias_name: &AliasName) -> Self {
        Self::NameResolution(NameResolution::duplicate_alias(alias_name))
    }

    /// accessing member not found in complex type
    #[track_caller]
    #[tracing::instrument()]
    pub fn composite_type_field_not_found(
        composite_type: &TypeReference,
        field_name: &str,
    ) -> Self {
        tracing::warn!(
            "complex type member not found {composite_type:?} member_name: {field_name}"
        );
        Self::NameResolution(NameResolution::composite_type_field_not_found(
            composite_type,
            field_name,
        ))
    }

    /// same column name existing from clause
    #[tracing::instrument()]
    pub fn ambiguous_column(column: &ColumnName) -> Self {
        Self::NameResolution(NameResolution::ambiguous_column(column))
    }

    /// alter table not found
    #[tracing::instrument()]
    pub fn table_not_found(table_name: &TableName) -> Self {
        Self::NameResolution(NameResolution::table_not_found(table_name))
    }

    /// sequence not found
    #[tracing::instrument()]
    pub fn sequence_not_found(seq_name: &FullName) -> Self {
        Self::NameResolution(NameResolution::sequence_not_found(seq_name))
    }

    /// specified index not found
    #[tracing::instrument()]
    pub fn index_not_found(index_name: &str) -> Self {
        Self::NameResolution(NameResolution::index_not_found(index_name))
    }

    /// specified view not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn view_not_found(view_name: &ViewName) -> Self {
        Self::NameResolution(NameResolution::view_not_found(view_name))
    }

    /// domain not found
    #[tracing::instrument()]
    pub fn domain_type_name_not_found(
        domain_name: &FullName,
        type_name: &crate::syn::TypeName,
    ) -> Self {
        Self::NameResolution(NameResolution::domain_type_name_not_found(
            domain_name,
            type_name,
        ))
    }

    /// specified table not found
    #[tracing::instrument()]
    pub fn relation_not_found(table_name: &TableName) -> Self {
        Self::NameResolution(NameResolution::relation_not_found(table_name))
    }

    /// calling function not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn function_not_found(func_name: &FullName) -> Self {
        Self::NameResolution(NameResolution::function_not_found(func_name))
    }

    /// specified table function not found
    #[track_caller]
    #[tracing::instrument()]
    pub fn table_function_not_found(func: &FullName) -> Self {
        Self::NameResolution(NameResolution::table_function_not_found(func))
    }

    /// table column type
    #[tracing::instrument()]
    pub fn column_type_not_found(
        table_name: &TableName,
        colname: &ColumnName,
        type_name: &FullName,
    ) -> Self {
        Self::NameResolution(NameResolution::column_type_not_found(
            table_name, colname, type_name,
        ))
    }

    /// undefined alias
    pub fn undefined_alias(alias_name: String) -> Self {
        Self::NameResolution(NameResolution::undefined_alias(&alias_name))
    }
}
