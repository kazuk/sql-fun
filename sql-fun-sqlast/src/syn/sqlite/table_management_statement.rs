use crate::syn::sqlite::{
    ColumnConstraint, ColumnDefinition, Name, QualifiedName, Select, TableConstraint,
};

use turso_parser::ast::AlterTableBody as AlterTableBodyIn;

/// alter table commands
#[derive(Debug)]
pub enum AlterTableBody {
    /// rename table
    RenameTo(Name),
    /// add column
    AddColumn(ColumnDefinition),
    /// alter column
    AlterColumn(Name, ColumnDefinition),
    /// rename column
    RenameColumn(Name, Name),
    /// drop column
    DropColumn(Name),
}

impl From<AlterTableBodyIn> for AlterTableBody {
    fn from(value: AlterTableBodyIn) -> Self {
        match value {
            AlterTableBodyIn::RenameTo(name) => Self::RenameTo(Name::from(name)),
            AlterTableBodyIn::AddColumn(column_definition) => {
                Self::AddColumn(ColumnDefinition::from(column_definition))
            }
            AlterTableBodyIn::AlterColumn { old, new } => {
                Self::AlterColumn(Name::from(old), ColumnDefinition::from(new))
            }
            AlterTableBodyIn::RenameColumn { old, new } => {
                Self::RenameColumn(Name::from(old), Name::from(new))
            }
            AlterTableBodyIn::DropColumn(name) => Self::DropColumn(Name::from(name)),
        }
    }
}

use turso_parser::ast::AlterTable as AlterTableIn;

/// alter table statement
#[derive(Debug)]
pub struct AlterTable {
    name: QualifiedName,
    body: AlterTableBody,
}

impl AlterTable {
    /// create instance
    pub fn name(&self) -> &QualifiedName {
        &self.name
    }

    /// get body
    pub fn body(&self) -> &AlterTableBody {
        &self.body
    }
}

impl From<AlterTableIn> for AlterTable {
    fn from(value: AlterTableIn) -> Self {
        Self {
            name: QualifiedName::from(value.name),
            body: AlterTableBody::from(value.body),
        }
    }
}

/// create table statement
#[derive(Debug, derive_getters::Getters, derive_getters::Dissolve)]
pub struct CreateTableStatement {
    temporary: bool,
    if_not_exists: bool,
    table_name: QualifiedName,
    body: CreateTableBody,
}

impl CreateTableStatement {
    /// create instance
    pub fn new(
        temporary: bool,
        if_not_exists: bool,
        table_name: QualifiedName,
        body: CreateTableBody,
    ) -> Self {
        Self {
            temporary,
            if_not_exists,
            table_name,
            body,
        }
    }
}

use turso_parser::ast::CreateTableBody as CreateTableBodyIn;

/// create table body
#[derive(Debug)]
pub enum CreateTableBody {
    /// columns and constraints definitions
    ColumnsAndConstraint(ColumnsAndConstraint),
    /// create table from query
    AsSelect(Select),
}

impl CreateTableBody {
    /// get ColumnsAndConstraint valiant
    pub fn as_columns_and_constraint(&self) -> Option<&ColumnsAndConstraint> {
        match self {
            Self::ColumnsAndConstraint(v) => Some(v),
            _ => None,
        }
    }
}

impl From<CreateTableBodyIn> for CreateTableBody {
    fn from(value: CreateTableBodyIn) -> Self {
        match value {
            CreateTableBodyIn::ColumnsAndConstraints {
                columns,
                constraints,
                options,
            } => Self::ColumnsAndConstraint(ColumnsAndConstraint::new(
                columns.into_iter().map(ColumnDefinition::from).collect(),
                constraints
                    .into_iter()
                    .map(NamedTableConstraint::from)
                    .collect(),
                TableOptions::from(options),
            )),
            CreateTableBodyIn::AsSelect(select) => Self::AsSelect(Select::from(select)),
        }
    }
}

/// column and constraint definitions
#[derive(Debug, derive_getters::Getters)]
pub struct ColumnsAndConstraint {
    columns: Vec<ColumnDefinition>,
    constraints: Vec<NamedTableConstraint>,
    options: TableOptions,
}

impl ColumnsAndConstraint {
    /// create instance
    pub fn new(
        columns: Vec<ColumnDefinition>,
        constraints: Vec<NamedTableConstraint>,
        options: TableOptions,
    ) -> Self {
        Self {
            columns,
            constraints,
            options,
        }
    }
}

use turso_parser::ast::NamedTableConstraint as NamedTableConstraintIn;

/// table constraint with name
#[derive(Debug, derive_getters::Getters)]
pub struct NamedTableConstraint {
    name: Option<Name>,
    constraint: TableConstraint,
}

impl From<NamedTableConstraintIn> for NamedTableConstraint {
    fn from(value: NamedTableConstraintIn) -> Self {
        Self {
            name: value.name.map(Name::from),
            constraint: TableConstraint::from(value.constraint),
        }
    }
}

use turso_parser::ast::NamedColumnConstraint as NamedColumnConstraintIn;

/// column constraint with name
#[derive(Debug)]
pub struct NamedColumnConstraint {
    name: Option<Name>,
    constraint: ColumnConstraint,
}

impl From<NamedColumnConstraintIn> for NamedColumnConstraint {
    fn from(value: NamedColumnConstraintIn) -> Self {
        Self {
            name: value.name.map(Name::from),
            constraint: ColumnConstraint::from(value.constraint),
        }
    }
}

impl NamedColumnConstraint {
    /// name
    pub fn name(&self) -> Option<&Name> {
        self.name.as_ref()
    }

    /// constraint
    pub fn constraint(&self) -> &ColumnConstraint {
        &self.constraint
    }
}

use turso_parser::ast::TableOptions as TableOptionsIn;

/// table options
#[derive(Debug, derive_getters::Getters)]
pub struct TableOptions {
    without_rowid_text: Option<String>,
    strict_text: Option<String>,
}

impl From<TableOptionsIn> for TableOptions {
    fn from(value: TableOptionsIn) -> Self {
        Self {
            without_rowid_text: value.without_rowid_text,
            strict_text: value.strict_text,
        }
    }
}

/// drop table statement
#[derive(Debug, derive_getters::Getters)]
pub struct DropTableStatement {
    if_exists: bool,
    name: QualifiedName,
}

impl DropTableStatement {
    /// create instance
    pub fn new(if_exists: bool, name: QualifiedName) -> Self {
        Self { if_exists, name }
    }
}

use turso_parser::ast::CreateVirtualTable as CreateVirtualTableIn;

/// create virtual table statement
#[derive(derive_getters::Getters, Debug)]
pub struct CreateVirtualTable {
    if_not_exists: bool,
    table_name: QualifiedName,
    module_name: Name,
    args: Vec<String>,
}

impl From<CreateVirtualTableIn> for CreateVirtualTable {
    fn from(value: CreateVirtualTableIn) -> Self {
        Self {
            if_not_exists: value.if_not_exists,
            table_name: QualifiedName::from(value.tbl_name),
            module_name: Name::from(value.module_name),
            args: value.args,
        }
    }
}
