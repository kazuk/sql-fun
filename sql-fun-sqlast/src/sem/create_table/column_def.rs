use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnConstraint, ColumnName, FullName, ParseContext,
        SemScalarExpr, TableName, TypeReference,
    },
    syn::{ListOpt, Opt, ScanToken},
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
        mut context: TParseContext,
        table_name: &TableName,
        column_def: &crate::syn::ColumnDef,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
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
                table_name, &colname, &key,
            ))?;
            None
        };

        let constraints = if let Some(constraint_list) =
            column_def.get_constraints().map(|v| v.as_constraint())
        {
            let mut constraints = Vec::new();
            for constraint in constraint_list {
                let (c, new_context) = ColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    &colname,
                    &constraint,
                    tokens,
                )?;
                context = new_context;

                if let Some(fk) = c.as_foreign_key()
                    && let Some(col_type) = &type_ref
                    && let Some(pk_col_type) = fk.primary_key_column_type()
                    && col_type != pk_col_type
                {
                    context.report_problem(AnalysisProblem::foreign_key_column_type_mismatch(
                        table_name,
                        &colname,
                        col_type,
                        fk.primary_key_table(),
                        fk.primary_key_column(),
                        pk_col_type,
                    ))?;
                }

                constraints.push(c)
            }
            constraints
        } else {
            Vec::default()
        };
        let column = Self::new(
            &Some(colname),
            type_ref.as_ref(),
            Some(is_not_null),
            &constraints,
        );
        Ok((column, context))
    }

    /// analyze [`crate::syn::sqlite::ColumnDefinition`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_column_definition<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        syn: &crate::syn::sqlite::ColumnDefinition,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let colname = ColumnName::from(syn.column_name());

        let col_type = if let Some(ct) = syn.column_type() {
            let type_name = FullName::from_local_name(ct.name());
            if let Some(_type_def) = context.get_type(&type_name) {
                Some(TypeReference::from_full_name(type_name))
            } else {
                context.report_problem(AnalysisProblem::column_type_not_found(
                    table_name, &colname, &type_name,
                ))?;
                None
            }
        } else {
            None
        };

        let mut result = Self::new(&Some(colname.clone()), col_type.as_ref(), None, &[]);

        for constraint in syn.constraints() {
            let (constraint, new_context) = ColumnConstraint::analyze_sqlite_column_constraint(
                context,
                table_name,
                &colname,
                constraint.constraint(),
                tokens,
            )?;
            context = new_context;

            match &constraint {
                ColumnConstraint::Nullability(nullable_constraint) => {
                    result.set_not_null(Some(nullable_constraint.is_not_null()));
                }
                ColumnConstraint::Default(default_constraint) => {
                    result.set_default(default_constraint.expr());
                }
                _ => {}
            }

            result.push_constraint(constraint);
        }

        Ok((result, context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_column_definition {
    use assert_matches::assert_matches;
    use sql_fun_core::IVec;

    use super::{ColumnConstraint, ColumnDefinition};
    use crate::{
        SqliteToken,
        sem::{
            FullName, ScalarConstExpr, SchemaName, SemScalarExpr, SqlValueExpr, TableName,
            TypeReference,
        },
        sqlite::SqliteDialect,
        syn::sqlite::{CreateTableBody, Stmt},
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
        let stmt = match cmd {
            turso_parser::ast::Cmd::Stmt(stmt) => Stmt::from(stmt),
            _ => panic!("expected sqlite statement command"),
        };
        let create_table = match stmt {
            Stmt::CreateTable(create_table) => create_table,
            _ => panic!("expected create table statement"),
        };
        (create_table, tokens)
    }

    fn setup_context() -> TestParseContext {
        let mut context = TestParseContext::default();
        context.set_get_search_path_result(&vec![SchemaName::from("public")]);
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

    #[test]
    fn analyze_sqlite_column_definition_collects_constraints() {
        let sql = r#"
            create table users (
                id integer primary key not null unique,
                name text default ('guest') collate nocase,
                active integer check (true and true),
                created_at text default current_timestamp)
        "#;
        let (create_table, tokens) = parse_create_table(sql);
        let body = match create_table.body() {
            CreateTableBody::ColumnsAndConstraint(body) => body,
            _ => panic!("expected column-based create table body"),
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
        match &name_col.constraints()[0] {
            ColumnConstraint::Default(default) => {
                let value = match default.expr() {
                    SemScalarExpr::Const(ScalarConstExpr::String(v)) => v,
                    _ => panic!("expect const string"),
                };
                assert_eq!(value, "guest");
            }
            _ => panic!("expected string default constraint"),
        }
        match &name_col.constraints()[1] {
            ColumnConstraint::Collate(collate) => {
                assert_eq!(collate.collation_name(), "nocase")
            }
            _ => panic!("expected collate constraint"),
        }

        let (active_col, new_context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[2],
            &tokens,
        )
        .expect("active column should analyze");
        context = new_context;
        assert_eq!(active_col.constraints().len(), 1);
        match &active_col.constraints()[0] {
            ColumnConstraint::Check(check_constraint) => {
                assert_matches!(check_constraint.expr(), SemScalarExpr::Boolean(_));
            }
            _ => panic!("expected check constraint"),
        }

        let (created_at_col, context) = ColumnDefinition::analyze_column_definition(
            context,
            &table_name,
            &body.columns()[3],
            &tokens,
        )
        .expect("created_at column should analyze");
        assert_eq!(created_at_col.constraints().len(), 1);
        match &created_at_col.constraints()[0] {
            ColumnConstraint::Default(default) => match default.expr() {
                SemScalarExpr::SqlValue(SqlValueExpr::CurrentTimestamp(None)) => {}
                _ => panic!("expected current_timestamp"),
            },
            _ => panic!("default constraint"),
        }
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
