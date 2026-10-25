use crate::sem::{AliasName, ColumnName, FullName, ParseContext, TableName, TypeReference};

/// A PostgreSQL system attribute bound to a table occurrence, not a declared column.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SystemColumnRef {
    alias: AliasName,
    column: ColumnName,
    attribute: ColumnName,
    table: TableName,
    column_type: TypeReference,
    source_nullable: bool,
}

impl SystemColumnRef {
    pub(in crate::sem) fn lookup_type(
        context: &impl ParseContext,
        column: &ColumnName,
    ) -> Option<TypeReference> {
        if context.dialect() != crate::ast::Dialect::postgres() {
            return None;
        }
        let name = match column.as_str() {
            "tableoid" => "oid",
            "ctid" => "tid",
            "xmin" | "xmax" => "xid",
            "cmin" | "cmax" => "cid",
            _ => return None,
        };
        Some(TypeReference::from_master_type_name(name))
    }

    pub(super) fn new(
        alias: &AliasName,
        column: &ColumnName,
        table: &TableName,
        column_type: &TypeReference,
        source_nullable: bool,
    ) -> Self {
        Self {
            alias: alias.clone(),
            column: column.clone(),
            attribute: column.clone(),
            table: table.clone(),
            column_type: column_type.clone(),
            source_nullable,
        }
    }

    /// Exact identity of the table providing this attribute.
    pub const fn relation(&self) -> &FullName {
        self.table.full_name()
    }

    /// Alias identifying the selected table occurrence.
    pub const fn alias(&self) -> &AliasName {
        &self.alias
    }

    /// Original system attribute, independent of projection or join renaming.
    pub const fn attribute(&self) -> &ColumnName {
        &self.attribute
    }

    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column = column.clone();
        self
    }
}

impl super::ColumnReference for SystemColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.column_type.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(!self.source_nullable)
    }

    fn get_column_def(&self) -> &Option<crate::sem::ColumnDefinition> {
        &None
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl SystemColumnRef {
    pub(super) const fn projection_reference(&self) -> Option<(&str, &crate::sem::ColumnName)> {
        Some((self.alias.as_str(), &self.column))
    }
}
