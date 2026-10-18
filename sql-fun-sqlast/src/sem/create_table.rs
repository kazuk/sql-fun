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
    ast::TryFromWithTokens,
    sem::{
        AlterTable, AnalysisError, AnalysisProblem, AstAndContextPair, Constraint, FullName,
        GetSourcePathWithSpan, ImplicitChange, ParseContext, SemAst, SourcePathWithSpan,
        alter_table::AlterObjSubCommand,
    },
    syn::{ListOpt, Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::CreateTableBody};

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

    fn apply_inline_primary_key_nullability(&mut self) {
        for column in &mut self.0 {
            column.apply_primary_key_nullability();
        }
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
    pub(crate) const fn if_not_exists(self) -> bool {
        matches!(self, Self::IfNotExists)
    }
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
#[derive(Debug, Clone)]
pub struct CreateTable {
    name: TableName,
    columns: ColumnCollection,
    triggers: crate::sem::TriggerReferences,
    alter_statements: Vec<AlterTable>,
    drop_statements: Vec<DropTable>,
    owner: Option<crate::sem::OwnerSpecification>,
    cluster_on: Option<String>,
    constraints: HashMap<String, Constraint>,
    mode: CreateTableMode,
    temporary: bool,
    source: SourcePathWithSpan,
}

#[cfg(test)]
mod test_create_table_clone {
    use crate::sem::{BaseParseContext, ColumnName, ParseContext, TableInfoWrite};
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn clone_and_catalog_insertion_preserve_primary_key_columns() -> testresult::TestResult {
        let table = test_helpers::table_with_primary_key()?;
        let id = ColumnName::from("id");
        assert_eq!(table.primary_key_columns(), vec![&id]);

        let cloned = table.clone();
        assert_eq!(cloned.primary_key_columns(), vec![&id]);

        let mut context = crate::test_helpers::schema_layer(
            Path::new("schema.sql"),
            &TestParseContext::default().freeze(),
        );
        context.insert_create_table(cloned.name().full_name(), &cloned);
        let inserted = context.get_table(cloned.name()).expect("inserted table");
        assert_eq!(inserted.primary_key_columns(), vec![&id]);
        Ok(())
    }

    mod test_helpers {
        use crate::sem::{
            AlterObjSubCommand, AlterTable, AnalysisError, ColumnDefinition, ColumnName,
            Constraint, CreateTable, CreateTableMode, FullName, PgBuiltInType,
            PrimaryKeyConstraint, TableName,
        };

        pub(super) fn table_with_primary_key() -> Result<CreateTable, AnalysisError> {
            let name = TableName::from(FullName::with_schema("public", "items"));
            let id = ColumnName::from("id");
            let source = crate::test_helpers::source_span();
            let mut table = CreateTable::new(
                &name,
                vec![ColumnDefinition::new(
                    &Some(id.clone()),
                    Some(&PgBuiltInType::int4()),
                    Some(true),
                    &[],
                )],
                CreateTableMode::from_if_not_exists(false),
                false,
                &source,
            );
            let primary_key = PrimaryKeyConstraint::new("items_pk", &name, &[id]);
            let constraint = Constraint::primary_key(&primary_key);
            let alteration = AlterTable::new(
                &name,
                &[AlterObjSubCommand::add_constraint(&constraint)],
                &source,
            );
            table.apply_alter(&alteration)?;
            Ok(table)
        }
    }
}

impl GetSourcePathWithSpan for CreateTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source
    }
}

impl CreateTable {
    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// Fully qualified trigger references associated with this relation.
    pub const fn triggers(&self) -> &crate::sem::TriggerReferences {
        &self.triggers
    }
    pub(crate) fn associate_trigger(&mut self, name: &crate::sem::FullName) {
        self.triggers.associate(name);
    }
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
            triggers: crate::sem::TriggerReferences::default(),
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

    pub(crate) fn declared_indexes(
        &self,
        name: &FullName,
    ) -> Result<Vec<crate::sem::CreateIndex>, AnalysisError> {
        let mut indexes = Vec::new();
        for constraint in self.constraints.values() {
            if !constraint.name().is_empty() {
                for change in constraint.implicit_changes()? {
                    indexes.push(change.into_index().with_table_identity(name));
                }
            }
        }
        Ok(indexes)
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
        let mut implicit_changes = Vec::new();
        for cmd in alter_table.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(new_owner) => {
                    if new_owner.can_apply() {
                        self.change_owner(new_owner);
                    }
                }
                AlterObjSubCommand::SetColumnDefault(column_name, sem_scalar_expr) => {
                    if let Some(column) = self.columns.get_column_mut(column_name) {
                        column.set_default(sem_scalar_expr);
                    }
                }
                AlterObjSubCommand::AddConstraint(constraint) => {
                    self.constraints
                        .insert(constraint.name().to_string(), constraint.clone());
                    implicit_changes.extend(constraint.implicit_changes()?);
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

        Ok(implicit_changes)
    }

    pub(crate) fn apply_primary_key_nullability(&mut self) {
        self.columns.apply_inline_primary_key_nullability();
        let keys: Vec<_> = self.primary_key_columns().into_iter().cloned().collect();
        for key in keys {
            if let Some(column) = self.columns.get_column_mut(&key) {
                column.set_not_null(Some(true));
            }
        }
    }

    pub(crate) fn record_declared_key(
        &mut self,
        constraint: &Constraint,
    ) -> Result<(), AnalysisError> {
        let key = if constraint.name().is_empty() {
            // An internal map slot identifies an unnamed declaration; it is never
            // exposed as a fabricated SQL constraint/index name.
            format!("<unnamed:{}>", self.constraints.len())
        } else {
            constraint.name().to_owned()
        };
        if self.constraints.contains_key(&key) {
            return Err(AnalysisError::unexpected_input_error(
                "duplicate constraint name",
            ));
        }
        self.constraints.insert(key, constraint.clone());
        Ok(())
    }

    /// Look up an explicitly named table constraint.
    pub(crate) fn constraint(&self, name: &str) -> Option<&Constraint> {
        self.constraints
            .get(name)
            .filter(|constraint| constraint.name() == name)
    }

    pub(crate) fn infer_insert_arbiters(
        &self,
        destination: &TableName,
        columns: &[ColumnName],
    ) -> Vec<crate::sem::InsertArbiter> {
        let mut arbiters = Vec::new();
        for constraint in self.constraints.values() {
            if constraint.matches_unique_columns(columns) {
                arbiters.push(crate::sem::InsertArbiter::table_constraint(
                    destination,
                    constraint,
                ));
            }
        }
        // Named inline keys also occur in the table constraint collection.
        // Use a column identity only when no table key already identifies it.
        if arbiters.is_empty()
            && columns.len() == 1
            && let Some(column) = self.columns.get_column_def(&columns[0])
            && column.has_unique_key()
        {
            arbiters.push(crate::sem::InsertArbiter::column_constraint(&columns[0]));
        }
        arbiters
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
        context: TParseContext,
        parent_schema: &Option<String>,
        create: crate::syn::CreateStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Table::try_from_with_tokens(&create, tokens)?
            .resolve_and_apply_at(context, source_span)
    }

    pub(crate) fn push_drop_statement(&mut self, drop_table: &DropTable) {
        self.drop_statements.push(drop_table.clone());
    }
}

#[cfg(feature = "sqlite")]
impl CreateTable {
    /// analyze [`crate::syn::sqlite::ColumnsAndConstraint`]
    #[expect(clippy::too_many_arguments)]
    pub fn analyze_columns_and_constraint<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        table_name: &TableName,
        mode: &CreateTableMode,
        temporary: bool,
        syn: crate::syn::sqlite::ColumnsAndConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax =
            crate::ast::Table::try_from(&syn)?.named(table_name.full_name(), *mode, temporary);
        let table = syntax.resolve(&mut context, source_span)?;
        Ok((table, context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_columns_and_constraint {
    use crate::{
        sem::{CreateTableMode, SchemaName, SourcePathWithSpan},
        syn::sqlite::{CreateTableBody, Stmt},
        test_helpers::{
            SqliteSynBuilder, TestParseContext, sem_builder, source_span, sqlite_syn, test_context,
        },
    };

    #[rstest::rstest]
    #[case(CreateTableMode::Default)]
    #[case(CreateTableMode::IfNotExists)]
    #[case(CreateTableMode::OrReplace)]
    fn preserves_explicit_creation_mode(
        mut test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
        source_span: SourcePathWithSpan,
        #[case] mode: CreateTableMode,
    ) -> testresult::TestResult {
        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let Stmt::CreateTable(table) = sqlite_syn.parse_as_stmt("create table users(id integer)")
        else {
            panic!("expected CREATE TABLE");
        };
        let (_, _, _, body) = table.dissolve();
        let CreateTableBody::ColumnsAndConstraint(body) = body else {
            panic!("expected column definitions");
        };
        let (table, _) = super::CreateTable::analyze_columns_and_constraint(
            test_context,
            &None,
            &sem_builder().table_name("users"),
            &mode,
            true,
            body,
            &Vec::new().into(),
            &source_span,
        )?;
        assert_eq!(table.mode(), &mode);
        assert!(table.temporary());
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
impl CreateTable {
    /// analyze [`crate::syn::sqlite::CreateTableBody`]
    #[expect(clippy::too_many_arguments)]
    pub fn analyze_create_table_body<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        table_name: &TableName,
        mode: &CreateTableMode,
        temporary: bool,
        syn: CreateTableBody,
        _tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Table::try_from(&syn)?
            .named(table_name.full_name(), *mode, temporary)
            .resolve_and_apply_at(context, source_span)
    }
}

#[cfg(feature = "sqlite")]
impl CreateTable {
    /// analyze [`crate::syn::sqlite::CreateTableStatement`]
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
        crate::ast::Table::try_from(&syn)?.resolve_and_apply_at(context, source_span)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_create_table {
    use assert_matches::assert_matches;
    use testresult::TestResult;

    use crate::{
        AstAndContextPair,
        sem::{BaseParseContext, CreateTableMode, SchemaName, SemAst, SourcePathWithSpan},
        syn::sqlite::Stmt,
        test_helpers::{
            SqliteSynBuilder, TestParseContext, sem_builder, source_span, sqlite_syn, test_context,
        },
    };

    #[rstest::rstest]
    #[case(
        "create table users(id integer, name text)",
        "users",
        CreateTableMode::Default,
        false
    )]
    #[case(
        "create table if not exists users(id integer, name text)",
        "users",
        CreateTableMode::IfNotExists,
        false
    )]
    #[case(
        "create temp table if not exists temp_test(id integer, name text)",
        "temp_test",
        CreateTableMode::IfNotExists,
        true
    )]
    fn test_analyze_create_table(
        mut test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
        source_span: SourcePathWithSpan,
        #[case] sql: &str,
        #[case] table_name: &str,
        #[case] table_mode: CreateTableMode,
        #[case] temporary: bool,
    ) -> TestResult {
        let sem_builder = sem_builder();
        // FIXME: PostgreSQL specific config require in SQLite tests
        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);

        let stmt = sqlite_syn.parse_as_stmt(sql);

        let Stmt::CreateTable(ct) = stmt else {
            panic!("expect CreateTable: {stmt:?}");
        };
        let tokens = Vec::default().into();
        let AstAndContextPair(sem_ast, result_context) = super::CreateTable::analyze_create_table(
            test_context,
            &None,
            ct,
            &tokens,
            &source_span,
        )?;
        assert_matches!(sem_ast, SemAst::CreateTable(_));
        let Some(create_table) = sem_ast.as_create_table() else {
            panic!("expect CreateTable")
        };
        assert_eq!(create_table.name(), &sem_builder.table_name(table_name));
        assert_eq!(create_table.mode(), &table_mode);
        assert_eq!(create_table.temporary(), temporary);

        let lookup_name = if temporary {
            crate::sem::TableName::from(crate::sem::FullName::with_schema("pg_temp", table_name))
        } else {
            sem_builder.table_name(table_name)
        };
        let Some(_table_in_context) = result_context.get_table(&lookup_name) else {
            panic!("table not found in context")
        };

        Ok(())
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
    /// Create a `DROP TABLE` statement.
    #[must_use]
    pub fn new(table_names: &[TableName], if_exists: bool, cascade: bool) -> Self {
        Self {
            if_exists,
            table_names: table_names.to_vec(),
            cascade,
        }
    }

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
                context.report_analysis_problem(AnalysisProblem::table_not_found(&table_name))?;
            }
            table_names.push(table_name);
        }

        let result = Self::new(&table_names, syn.get_missing_ok(), cascade);
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

impl DropTable {
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
            context.report_analysis_problem(AnalysisProblem::table_not_found(&table_name))?;
        }
        let result = Self::new(std::slice::from_ref(&table_name), *if_exists, false);
        context = context.apply_drop_table(&result)?;
        Ok(AstAndContextPair::new(SemAst::DropTable(result), context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_drop_table_analyze_drop_table {
    use crate::{
        AstAndContextPair,
        syn::sqlite::DropTableStatement,
        test_helpers::{SqliteSynBuilder, TestParseContext, sqlite_syn, test_context},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_drop_table(
        test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
    ) -> TestResult {
        let table_name = sqlite_syn.qualified_name("table_one");
        let statement = DropTableStatement::new(true, table_name);
        let tokens = Vec::default().into();
        let AstAndContextPair(ast, result_context) =
            super::DropTable::analyze_drop_table(test_context, statement, &tokens)?;
        let Some(drop_table) = ast.as_drop_table() else {
            panic!("{ast:?} is not DropTable")
        };
        assert!(drop_table.if_exists());
        let table_name = drop_table.table_names().first().unwrap();
        table_name.assert_table_name("table_one");
        assert_eq!(result_context.reported_problem_count(), 1);

        Ok(())
    }
}
