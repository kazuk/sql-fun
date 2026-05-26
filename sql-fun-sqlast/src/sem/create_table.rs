mod column_def;
mod column_name;
mod table_name;
mod view_name;

pub use self::{
    column_def::ColumnDefinition, column_name::ColumnName, table_name::TableName,
    view_name::ViewName,
};
use sql_fun_core::IVec;
use std::collections::HashMap;

use crate::{
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, AstAndContextPair, Constraint, FullName,
        GetSourcePathWithSpan, ImplicitChange, ParseContext, SemAst, SourcePathWithSpan,
        alter_table::AlterObjSubCommand,
    },
    syn::{ListOpt, Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::CreateTableBody};

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
    pub const fn new(columns: Vec<ColumnDefinition>) -> Self {
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

/// mode of `CREATE TABLE` statement
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CreateTableMode {
    /// create table mode default
    Default,
    /// create table with `IF NOT EXISTS`
    IfNotExists,
    /// create table with `OR REPLACE`
    ///
    /// # Note
    ///
    /// This valiant is not used in now, `PostgreSQL` and `SQLite` not supports `create table or replace`
    OrReplace,
}

impl CreateTableMode {
    /// create instance for if not exists
    pub const fn from_if_not_exists(if_not_exists: bool) -> Self {
        if if_not_exists {
            Self::IfNotExists
        } else {
            Self::Default
        }
    }
}

/// `create table` SQL statement
#[derive(Debug)]
pub struct CreateTable {
    name: TableName,
    columns: ColumnCollection,
    alter_statements: Vec<AlterTable>,
    drop_statements: Vec<DropTable>,
    owner: Option<String>,
    cluster_on: Option<String>,
    constraints: HashMap<String, Constraint>,
    mode: CreateTableMode,
    temporary: bool,
    source: SourcePathWithSpan,
}

impl Clone for CreateTable {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            columns: self.columns.clone(),
            alter_statements: self.alter_statements.clone(),
            drop_statements: self.drop_statements.clone(),
            owner: Default::default(),
            cluster_on: Default::default(),
            constraints: Default::default(),
            mode: self.mode,
            temporary: self.temporary,
            source: self.source.clone(),
        }
    }
}

impl GetSourcePathWithSpan for CreateTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source
    }
}

impl CreateTable {
    /// new instance with column definitions
    #[must_use]
    pub fn new(
        name: &TableName,
        columns: Vec<ColumnDefinition>,
        mode: CreateTableMode,
        temporary: bool,
        source: &SourcePathWithSpan,
    ) -> Self {
        Self {
            name: name.clone(),
            columns: ColumnCollection(columns),
            alter_statements: Default::default(),
            drop_statements: Default::default(),
            owner: Default::default(),
            cluster_on: Default::default(),
            constraints: Default::default(),
            mode,
            temporary,
            source: source.clone(),
        }
    }

    /// get table name
    #[must_use]
    pub const fn name(&self) -> &TableName {
        &self.name
    }

    /// get [`CreateTableMode`]
    pub const fn mode(&self) -> &CreateTableMode {
        &self.mode
    }

    /// is temporary table
    pub const fn temporary(&self) -> bool {
        self.temporary
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
                AlterObjSubCommand::RenameTo(_) => {
                    todo!()
                }
                AlterObjSubCommand::AddColumn(_) => {
                    todo!()
                }
                AlterObjSubCommand::AlterColumn(_, _) => {
                    todo!()
                }
                AlterObjSubCommand::RenameColumn(_, _) => {
                    todo!()
                }
                AlterObjSubCommand::DropColumn(_) => {
                    todo!()
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
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
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
            let (coldef, new_context) =
                ColumnDefinition::analyze_column_def(context, &table_name, &column_def, tokens)?;
            context = new_context;
            cols.push(coldef);
        }
        let table_def = Self::new(
            &table_name,
            cols,
            CreateTableMode::from_if_not_exists(create.get_if_not_exists()),
            false,
            source_span,
        );
        context = context.apply_create_table(&table_def)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateTable(table_def),
            context,
        ))
    }

    pub(crate) fn push_drop_statement(&mut self, drop_table: &DropTable) {
        self.drop_statements.push(drop_table.clone());
    }

    /// analyze [`crate::syn::sqlite::ColumnAndConstraint`]
    #[cfg(feature = "sqlite")]
    #[expect(clippy::too_many_arguments)]
    pub fn analyze_columns_and_constraint<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        table_name: &TableName,
        mode: &CreateTableMode,
        temporary: bool,
        syn: crate::syn::sqlite::ColumnsAndConstraint,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut cols = Vec::new();
        for col_def in syn.columns() {
            let (col, new_context) =
                ColumnDefinition::analyze_column_definition(context, table_name, col_def, tokens)?;
            context = new_context;
            cols.push(col);
        }

        let table_def = Self::new(table_name, cols, *mode, temporary, source_span);
        Ok((table_def, context))
    }

    /// analyze [`crate::syn::sqlite::CreateTableBody`]
    #[cfg(feature = "sqlite")]
    #[expect(clippy::too_many_arguments)]
    pub fn analyze_create_table_body<TParseContext>(
        context: TParseContext,
        parent_schema: &Option<String>,
        table_name: &TableName,
        mode: &CreateTableMode,
        temporary: bool,
        syn: CreateTableBody,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::syn::sqlite::CreateTableBody;

        match syn {
            CreateTableBody::ColumnsAndConstraint(columns_and_constraint) => {
                let (result, context) = Self::analyze_columns_and_constraint(
                    context,
                    parent_schema,
                    table_name,
                    mode,
                    temporary,
                    columns_and_constraint,
                    tokens,
                    source_span,
                )?;
                Ok(AstAndContextPair::new(SemAst::CreateTable(result), context))
            }
            CreateTableBody::AsSelect(_select) => todo!(),
        }
    }

    /// analyze [`crate::syn::sqlite::CreateTableStatement`]
    #[cfg(feature = "sqlite")]
    #[tracing::instrument(skip(context))]
    pub fn analyze_create_table<TParseContext>(
        context: TParseContext,
        parent_schema: &Option<String>,
        syn: crate::syn::sqlite::CreateTableStatement,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (temporary, if_not_exists, table_name, body) = syn.dissolve();
        let table_name = TableName::try_from(&table_name)?;
        let mode = CreateTableMode::from_if_not_exists(if_not_exists);

        Self::analyze_create_table_body(
            context,
            parent_schema,
            &table_name,
            &mode,
            temporary,
            body,
            tokens,
            source_span,
        )
    }
}

/// `DROP TABLE` statement
#[derive(Debug, Clone)]
pub struct DropTable {
    if_exists: bool,
    table_names: Vec<TableName>,
    cascade: bool,
}

impl DropTable {
    /// True when `IF EXISTS` or same meaning specified.
    pub const fn if_exists(&self) -> bool {
        self.if_exists
    }

    /// get target table name.
    pub fn table_names(&self) -> &[TableName] {
        &self.table_names
    }

    /// get cascade
    pub const fn cascade(&self) -> bool {
        self.cascade
    }

    /// get restrict
    pub const fn restrict(&self) -> bool {
        !self.cascade
    }
}

impl DropTable {
    ///
    /// analyze DropStmt as Drop Table for Postgres dialect
    ///
    /// # Panics
    ///
    /// When drop statement is not targeting table.
    ///
    pub fn analyze_drop_stmt<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::DropStmt,
        _tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        assert!(matches!(
            syn.get_remove_type().as_inner(),
            Some(crate::syn::ObjectType::ObjectTable)
        ));

        let Some(objects) = syn.get_objects().as_inner() else {
            AnalysisError::raise_unexpected_none("DropStmt.objects")?;
            AnalysisError::never();
        };
        let cascade = matches!(
            syn.get_behavior().as_inner(),
            Some(crate::syn::DropBehavior::DropCascade)
        );

        let mut table_names = Vec::new();
        for object in objects {
            let full_name = FullName::try_from(object.as_list().get_items())?;
            let table_name = TableName::from(full_name);
            if context.get_table(&table_name).is_none() {
                context.report_problem(AnalysisProblem::table_not_found(&table_name))?;
            }
            table_names.push(table_name);
        }

        let result = Self {
            if_exists: syn.get_missing_ok(),
            table_names,
            cascade,
        };
        context = context.apply_drop_table(&result)?;
        Ok(AstAndContextPair::new(SemAst::DropTable(result), context))
    }

    ///
    /// analyze DropTableStatemtn for SQLite dialect
    ///
    #[cfg(feature = "sqlite")]
    pub fn analyze_drop_table<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::sqlite::DropTableStatement,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let if_exists = syn.if_exists();
        let table_name = TableName::try_from(syn.name())?;
        if context.get_table(&table_name).is_none() {
            context.report_problem(AnalysisProblem::table_not_found(&table_name))?;
        }
        let result = Self {
            if_exists: *if_exists,
            table_name: table_name.to_owned(),
        };
        context = context.apply_drop_table(&result)?;
        Ok(AstAndContextPair::new(SemAst::DropTable(result), context))
    }
}

#[cfg(test)]
mod test_drop_table_analyze_drop_stmt {
    use testresult::TestResult;

    use crate::{
        AstAndContextPair,
        sem::DropTable,
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    #[case("drop table table_one", false, false)]
    #[case("drop table if exists table_one", true, false)]
    #[case("drop table table_one cascade", false, true)]
    #[case("drop table table_one restrict", false, false)]
    fn test_analyze_drop_stmt(
        test_context: TestParseContext,
        #[case] sql: &str,
        #[case] if_exists: bool,
        #[case] cascade: bool,
    ) -> TestResult {
        let builder = SynBuilder::new();
        let Some(node) = builder.parse(sql).as_drop_stmt().as_inner() else {
            panic!("expect DropStmt");
        };
        let tokens = Vec::default().into();
        let AstAndContextPair(ast, _result_context) =
            DropTable::analyze_drop_stmt(test_context, node, &tokens)?;
        let Some(drop_table) = ast.as_drop_table() else {
            panic!("{ast:?} is not DropTable");
        };
        assert_eq!(drop_table.if_exists(), if_exists);
        assert_eq!(drop_table.cascade(), cascade);
        let table_name = drop_table.table_names.first().unwrap();
        table_name.assert_table_name("table_one");

        Ok(())
    }
}
