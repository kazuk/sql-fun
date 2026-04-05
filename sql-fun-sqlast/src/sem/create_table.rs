mod column_def;
mod column_name;
mod table_name;
mod view_name;

pub use self::{
    column_def::ColumnDefinition, column_name::ColumnName, table_name::TableName,
    view_name::ViewName,
};
use std::collections::HashMap;

use crate::{
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, AstAndContextPair, Constraint, FullName,
        ImplicitChange, ParseContext, SemAst, alter_table::AlterObjSubCommand,
    },
    syn::{ListOpt, Opt},
};

#[macro_use]
#[path = "./macros.rs"]
mod macros;

impl_from_ref!(TableName);
impl_from_ref!(ColumnName);
impl_from_ref!(ViewName);

/// columns in table
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnCollection(Vec<ColumnDefinition>);

impl ColumnCollection {
    /// add column
    #[must_use]
    pub fn new(columns: Vec<ColumnDefinition>) -> Self {
        Self(columns)
    }

    /// find mutateble column by name
    pub fn get_column_mut(&mut self, column_name: &ColumnName) -> Option<&mut ColumnDefinition> {
        self.0
            .iter_mut()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    /// get column definition by name
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<&ColumnDefinition> {
        self.0
            .iter()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    /// test column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.0.iter().any(|c| c.name().as_ref() == Some(column))
    }

    /// get column definition by index
    #[must_use]
    pub fn get_at(&self, column_index: usize) -> Option<&ColumnDefinition> {
        self.0.get(column_index)
    }

    /// get column definition with skip count
    #[must_use]
    pub fn get_column_def_with_skip_count(
        &self,
        column_name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ColumnDefinition> {
        self.0[skip_count..]
            .iter()
            .find(|c| c.name().as_ref() == Some(column_name))
    }

    /// append column definition
    pub fn push(&mut self, column: &ColumnDefinition) {
        self.0.push(column.clone());
    }
}

/// `create table` SQL statement
#[derive(Debug)]
pub struct CreateTable {
    name: TableName,
    columns: ColumnCollection,
    alter_statements: Vec<AlterTable>,
    owner: Option<String>,
    cluster_on: Option<String>,
    constraints: HashMap<String, Constraint>,
}

impl Clone for CreateTable {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            columns: self.columns.clone(),
            alter_statements: Default::default(),
            owner: Default::default(),
            cluster_on: Default::default(),
            constraints: Default::default(),
        }
    }
}

impl CreateTable {
    /// new instance with column definitions
    #[must_use]
    pub fn new(name: TableName, columns: Vec<ColumnDefinition>) -> Self {
        Self {
            name,
            columns: ColumnCollection(columns),
            alter_statements: Default::default(),
            owner: Default::default(),
            cluster_on: Default::default(),
            constraints: Default::default(),
        }
    }

    /// get table name
    #[must_use]
    pub fn name(&self) -> &TableName {
        &self.name
    }

    /// get column definition by column name
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<&ColumnDefinition> {
        self.columns.get_column_def(column_name)
    }

    /// Look up a column definition by *original* column name, ignoring the first `skip_count` columns.
    ///
    /// This is used to implement `PostgreSQL`'s behavior for a table alias with a column alias list:
    ///
    /// ```sql
    /// SELECT ... FROM users AS u(id, name);
    /// ```
    ///
    /// When a column alias list is present, the original names of the first N columns (N = the length
    /// of the alias list) are not visible through that alias. Only the alias names are visible for
    /// those columns. Columns after the first N keep their original names.
    ///
    /// For example, if `users` has at least two columns and the 1st column is named `user_id`,
    /// then `u.user_id` is not accessible when `skip_count >= 1` (e.g. `u(id, name)` makes `skip_count = 2`).
    /// However, columns after the first `skip_count` columns are still accessible by their original names.
    ///
    #[must_use]
    pub fn get_column_def_with_skip_count(
        &self,
        column_name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ColumnDefinition> {
        self.columns
            .get_column_def_with_skip_count(column_name, skip_count)
    }

    /// get column definition by index
    #[must_use]
    pub fn get_column_at(&self, column_index: usize) -> Option<&ColumnDefinition> {
        self.columns.get_at(column_index)
    }

    /// check column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.columns.has_column(column)
    }

    /// apply `alter table` statement
    pub fn apply_alter(
        &mut self,
        alter_table: &AlterTable,
    ) -> Result<Vec<ImplicitChange>, AnalysisError> {
        self.alter_statements.push(alter_table.clone());

        for cmd in alter_table.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(new_owner) => self.owner = Some(new_owner.clone()),
                AlterObjSubCommand::SetColumnDefault(column_name, sem_scalar_expr) => {
                    if let Some(column) = self.columns.get_column_mut(column_name) {
                        column.set_default(sem_scalar_expr);
                    }
                }
                AlterObjSubCommand::AddConstraint(constraint) => {
                    self.constraints
                        .insert(constraint.name().to_string(), constraint.clone());
                    return constraint.implicit_changes();
                }
                AlterObjSubCommand::ClusterOn(index_name) => {
                    self.cluster_on = Some(index_name.clone())
                }
            }
        }

        Ok(Vec::new())
    }

    /// get primary key column names
    #[must_use]
    pub fn primary_key_columns(&self) -> Vec<&ColumnName> {
        if let Some(pk) = self
            .constraints
            .iter()
            .find_map(|(_, c)| c.as_primary_key_constraint())
        {
            pk.keys().iter().collect()
        } else {
            Vec::new()
        }
    }

    /// analyze [`crate::syn::CreateStmt`]
    #[tracing::instrument(skip(context))]
    pub fn analyze_create_stmt<TParseContext>(
        mut context: TParseContext,
        parent_schema: &Option<String>,
        create: crate::syn::CreateStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let table_name = create.get_relation().unwrap();
        let table_name = TableName::try_from(table_name)?;
        let Some(column_defs) = create.get_table_elts().map(|f| f.as_column_def()) else {
            AnalysisError::raise_unexpected_none("create_stmt.get_table_elts")?
        };
        let mut cols: Vec<ColumnDefinition> = Vec::new();
        for column_def in column_defs {
            let colname = column_def.get_colname();
            let colname = ColumnName::new(&colname);
            let Some(type_name) = column_def.get_type_name().as_inner() else {
                AnalysisError::raise_unexpected_none("column_def.get_type_name")?
            };
            let is_not_null = column_def.get_is_not_null();

            let key = FullName::try_from(type_name)?;
            let type_ref = if let Some(sem_type) = context.get_type(&key).cloned() {
                if let Some(type_ref) = sem_type.type_reference().cloned() {
                    Some(type_ref)
                } else {
                    context.report_problem(AnalysisProblem::unexpected_dynamic_type(&sem_type))?;
                    None
                }
            } else {
                context.report_problem(AnalysisProblem::column_type_not_found(
                    &table_name,
                    &colname,
                    &key,
                ))?;
                None
            };

            cols.push(ColumnDefinition::new(
                &Some(colname),
                type_ref.as_ref(),
                Some(is_not_null),
            ));
        }
        let table_def = Self::new(table_name, cols);
        let result_context = context.apply_create_table(&table_def)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateTable(table_def),
            result_context,
        ))
    }
}

/// analyze [`crate::syn::CreateStmt`]
#[deprecated = "use CreateTable::analyze_create_stmt"]
pub fn analyze_create_stmt<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    create: crate::syn::CreateStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateTable::analyze_create_stmt(context, parent_schema, create)
}
