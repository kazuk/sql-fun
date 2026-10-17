use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    sem::{
        AnalysisError, ColumnConstraint, ColumnName, ParseContext, SemScalarExpr, TableName,
        TypeReference,
    },
    syn::ScanToken,
};

#[cfg(feature = "sqlite")]
use crate::SqliteToken;

/// column definitions
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    name: Option<ColumnName>,
    type_ref: Option<TypeReference>,
    #[deprecated = "query from constraints"]
    is_not_null: Option<bool>,
    #[deprecated = "query from constraints"]
    default: Option<Box<SemScalarExpr>>,
    constraints: Vec<ColumnConstraint>,
}

impl Clone for ColumnDefinition {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            type_ref: self.type_ref.clone(),
            #[expect(deprecated)]
            is_not_null: self.is_not_null,
            #[expect(deprecated)]
            default: self.default.clone(),
            constraints: self.constraints.clone(),
        }
    }
}

impl ColumnDefinition {
    /// create instance
    #[must_use]
    pub fn new(
        name: &Option<ColumnName>,
        type_ref: Option<&TypeReference>,
        is_not_null: Option<bool>,
        constraints: &[ColumnConstraint],
    ) -> Self {
        Self {
            name: name.clone(),
            type_ref: type_ref.cloned(),
            #[expect(deprecated)]
            is_not_null,
            #[expect(deprecated)]
            default: Default::default(),
            constraints: constraints.to_vec(),
        }
    }

    /// get name of column
    #[must_use]
    pub const fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type_cloned(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// get [`TypeReference`]
    #[must_use]
    pub const fn get_type(&self) -> Option<&TypeReference> {
        self.type_ref.as_ref()
    }

    /// column nullability
    #[must_use]
    #[expect(deprecated)]
    pub const fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }

    /// apply default expression to column
    #[expect(deprecated)]
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()));
    }

    /// apply not null constraint to column
    #[expect(deprecated)]
    pub const fn set_not_null(&mut self, not_null: Option<bool>) {
        self.is_not_null = not_null;
    }

    /// get column constraints
    #[must_use]
    pub const fn constraints(&self) -> &[ColumnConstraint] {
        self.constraints.as_slice()
    }

    /// add column constraint
    pub fn push_constraint(&mut self, constraint: ColumnConstraint) {
        self.constraints.push(constraint)
    }

    /// analyze [`crate::syn::ColumnDef`]
    pub fn analyze_column_def<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_def: &crate::syn::ColumnDef,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Column::try_from_with_tokens(column_def, tokens)?
            .resolve_and_report(context, table_name)
    }

    /// analyze [`crate::syn::sqlite::ColumnDefinition`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_column_definition<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        syn: &crate::syn::sqlite::ColumnDefinition,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::Column::try_from(syn)?.resolve_and_report(context, table_name)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_column_definition {
    use assert_matches::assert_matches;
    use sql_fun_core::IVec;

    use super::{ColumnConstraint, ColumnDefinition};
    use crate::{
        SqliteToken,
        sem::{FullName, SchemaName, SemScalarExpr, TableName, TypeReference},
        sqlite::SqliteDialect,
        syn::sqlite::CreateTableBody,
        test_helpers::TestParseContext,
    };

    fn parse_create_table<'a>(
        sql: &'a str,
    ) -> (
        crate::syn::sqlite::CreateTableStatement,
        IVec<SqliteToken<'a>>,
    ) {
        let cmd = SqliteDialect::parse(sql)
            .expect("sqlite parse should succeed")
            .expect("statement should exist");
        let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
        let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
        let Some(create_table) = stmt.into_create_table() else {
            panic!("expected create table statement")
        };
        (create_table, tokens)
    }

    fn setup_context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context.setup_type(TypeReference::concrete_type_ref(
            &FullName::with_schema("public", "integer"),
            false,
        ));
        context.setup_type(TypeReference::concrete_type_ref(
            &FullName::with_schema("public", "text"),
            false,
        ));
        context
    }

    #[rstest::fixture]
    fn create_table_statement<'a>() -> (
        crate::syn::sqlite::CreateTableStatement,
        IVec<SqliteToken<'a>>,
    ) {
        let sql = r#"
            create table users (
                id integer primary key not null unique,
                name text default ('guest') collate nocase,
                active integer check (true and true),
                created_at text default current_timestamp)
        "#;
        let cmd = SqliteDialect::parse(sql)
            .expect("sqlite parse should succeed")
            .expect("statement should exist");
        let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
        let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
        let Some(create_table) = stmt.into_create_table() else {
            panic!("expected create table statement")
        };
        (create_table, tokens)
    }

    #[rstest::rstest]
    fn analyze_sqlite_column_definition_collects_constraints<'a>(
        create_table_statement: (
            crate::syn::sqlite::CreateTableStatement,
            IVec<SqliteToken<'a>>,
        ),
    ) {
        let (create_table, tokens) = create_table_statement;
        let Some(body) = create_table.body().as_columns_and_constraint() else {
            panic!("expected column-based create table body");
        };

        let table_name = TableName::try_from(create_table.table_name())
            .expect("table name should convert into semantic table name");
        let mut context = setup_context();

        let (id_col, new_context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[0],
            &tokens,
        )
        .expect("id column should analyze");
        context = new_context;
        assert_eq!(id_col.constraints().len(), 3);
        assert_matches!(id_col.constraints()[0], ColumnConstraint::PrimaryKey(_));
        assert_matches!(id_col.constraints()[1], ColumnConstraint::Nullability(_));
        assert_matches!(id_col.constraints()[2], ColumnConstraint::Unique(_));
        assert_eq!(id_col.is_not_null(), Some(true));

        let (name_col, new_context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[1],
            &tokens,
        )
        .expect("name column should analyze");
        context = new_context;
        assert_eq!(name_col.constraints().len(), 2);
        let Some(default) = &name_col.constraints()[0].as_default() else {
            panic!("expected string default constraint");
        };
        let Some(const_value) = default.expr().as_const() else {
            panic!("expect const string")
        };
        let Some(value) = const_value.as_string() else {
            panic!("expect const string")
        };
        assert_eq!(value, "guest");

        let Some(collate) = &name_col.constraints()[1].as_collate() else {
            panic!("expected collate constraint")
        };
        assert_eq!(collate.collation_name(), "nocase");

        let (active_col, new_context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[2],
            &tokens,
        )
        .expect("active column should analyze");
        context = new_context;
        assert_eq!(active_col.constraints().len(), 1);

        let Some(check_constraint) = &active_col.constraints()[0].as_check() else {
            panic!("expected check constraint")
        };
        assert_matches!(check_constraint.expr(), SemScalarExpr::Boolean(_));

        let (created_at_col, context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[3],
            &tokens,
        )
        .expect("created_at column should analyze");
        assert_eq!(created_at_col.constraints().len(), 1);
        let Some(default) = &created_at_col.constraints()[0].as_default() else {
            panic!("default constraint")
        };
        let Some(sql_value) = default.expr().as_sql_value() else {
            panic!("expected SqlValue")
        };
        let Ok(time_prec) = sql_value.as_current_timestamp() else {
            panic!("expected current_timestamp")
        };
        assert!(time_prec.is_none());

        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn analyze_sqlite_column_definition_reports_unknown_collation() {
        let sql = "create table users (name text collate custom)";
        let (create_table, tokens) = parse_create_table(sql);
        let body = match create_table.body() {
            CreateTableBody::ColumnsAndConstraint(body) => body,
            _ => panic!("expected column-based create table body"),
        };
        let table_name = TableName::try_from(create_table.table_name())
            .expect("table name should convert into semantic table name");

        let (column_def, context) = ColumnDefinition::analyze_column_definition(
            setup_context(),
            &table_name,
            &body.columns()[0],
            &tokens,
        )
        .expect("column definition should analyze");

        assert_eq!(column_def.constraints().len(), 1);
        match &column_def.constraints()[0] {
            ColumnConstraint::Collate(collate) => {
                assert_eq!(collate.collation_name(), "custom")
            }
            _ => panic!("expected collate constraint"),
        }
        assert_eq!(context.reported_problem_count(), 1);
    }
}
