use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnName, FromClause, FullName, ParseContext,
        SemScalarExpr, TableName, TypeReference, WithClause,
    },
    syn::{ConstrType, ListOpt, Opt, ScanToken},
};

#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::ResolveType};

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
    pub fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// column nullability
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        #[expect(deprecated)]
        self.is_not_null
    }

    /// apply default expression to column
    #[expect(deprecated)]
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()));
    }

    /// apply not null constrant to column
    #[expect(deprecated)]
    pub fn set_not_null(&mut self, not_null: Option<bool>) {
        self.is_not_null = not_null;
    }

    /// get column constraints
    #[must_use]
    pub fn constraints(&self) -> &[ColumnConstraint] {
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
                let (c, new_context) =
                    ColumnConstraint::analyze_constraint(context, table_name, &constraint, tokens)?;
                context = new_context;
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

        let mut result = Self::new(&Some(colname), col_type.as_ref(), None, &[]);

        for constraint in syn.constraints() {
            let (constraint, new_context) = ColumnConstraint::analyze_sqlite_column_constraint(
                context,
                table_name,
                constraint.constraint(),
                tokens,
            )?;
            context = new_context;

            match &constraint {
                ColumnConstraint::NotNull(nullable_constraint) => {
                    result.set_not_null(Some(nullable_constraint.is_not_null));
                }
                ColumnConstraint::Default(default_constraint) => {
                    result.set_default(&default_constraint.expr);
                }
                _ => {}
            }

            result.push_constraint(constraint);
        }

        Ok((result, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ConflictClause {
    /// Rollback
    Rollback,
    /// Abort
    Abort,
    /// fail
    Fail,
    /// ignore
    Ignore,
    /// replace
    Replace,
}

#[cfg(feature = "sqlite")]
impl From<&ResolveType> for ConflictClause {
    fn from(value: &ResolveType) -> Self {
        match value {
            ResolveType::Rollback => Self::Rollback,
            ResolveType::Abort => Self::Abort,
            ResolveType::Fail => Self::Fail,
            ResolveType::Ignore => Self::Ignore,
            ResolveType::Replace => Self::Replace,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrable {
    init_deferred: bool,
}

impl Deferrable {
    pub fn try_from_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let r = if syn.get_deferrable() {
            Some(Self {
                init_deferred: syn.get_initdeferred(),
            })
        } else {
            if syn.get_initdeferred() {
                AnalysisError::raise_unexpected_input("defferable: false, but initdeferred")?;
            }
            None
        };
        Ok(r)
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Validity {
    skip_validation: bool,
    initially_valid: bool,
}

impl Validity {
    pub fn from_constraint(syn: &crate::syn::Constraint) -> Self {
        Self {
            skip_validation: syn.get_skip_validation(),
            initially_valid: syn.get_initially_valid(),
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct PrimaryKeyColumnConstraint {
    conflict_clause: Option<ConflictClause>,
    deferrable: Option<Deferrable>,
}

impl PrimaryKeyColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrPrimary) {
            AnalysisError::raise_unexpected_input("contype not primary key")?;
            AnalysisError::never()
        }

        Ok((
            Self {
                conflict_clause: None,
                deferrable: Deferrable::try_from_constraint(syn)?,
            },
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::PrimaryKeyColumnConstraint> for PrimaryKeyColumnConstraint {
    fn from(value: &crate::syn::sqlite::PrimaryKeyColumnConstraint) -> Self {
        Self {
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
            deferrable: None,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ColumnNullableConstraint {
    is_not_null: bool,
    conflict_clause: Option<ConflictClause>,
    validity: Option<Validity>,
}

impl ColumnNullableConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        let not_null = if matches!(contype, ConstrType::ConstrNotnull) {
            true
        } else if matches!(contype, ConstrType::ConstrNull) {
            false
        } else {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        };
        Ok((
            Self {
                is_not_null: not_null,
                conflict_clause: None,
                validity: Some(Validity::from_constraint(syn)),
            },
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::NotNullColumnConstraint> for ColumnNullableConstraint {
    fn from(value: &crate::syn::sqlite::NotNullColumnConstraint) -> Self {
        Self {
            is_not_null: !value.nullable(),
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
            validity: None,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct UniqueColumnConstraint {
    conflict_clause: Option<ConflictClause>,
}

impl UniqueColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("contraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrUnique) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        }

        Ok((
            Self {
                conflict_clause: None,
            },
            context,
        ))
    }
}

#[cfg(feature = "sqlite")]
impl From<&crate::syn::sqlite::UniqueColumnConstraint> for UniqueColumnConstraint {
    fn from(value: &crate::syn::sqlite::UniqueColumnConstraint) -> Self {
        Self {
            conflict_clause: value.conflict_clause().map(ConflictClause::from),
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CheckColumnConstraint {
    expr: SemScalarExpr,
}

impl CheckColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrCheck) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        }
        let Some(check_expr) = syn.get_raw_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.raw_expr")?;
            AnalysisError::never()
        };

        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            check_expr,
            tokens,
        )?;
        Ok((Self { expr }, new_context))
    }

    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::CheckColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            syn.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((Self { expr }, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    expr: SemScalarExpr,
}

impl DefaultColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        if !matches!(contype, ConstrType::ConstrDefault) {
            AnalysisError::raise_unexpected_input("constraint.contype")?;
            AnalysisError::never()
        };

        let Some(default_expr) = syn.get_raw_expr().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.raw_expr")?;
            AnalysisError::never()
        };
        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            default_expr,
            tokens,
        )?;
        context = new_context;
        Ok((Self { expr }, context))
    }

    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::DefaultColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (expr, new_context) = SemScalarExpr::analyze_sqlite_scaler_expr(
            context,
            &WithClause::default(),
            &FromClause::default(),
            syn.expr(),
            tokens,
        )?;
        context = new_context;
        Ok((Self { expr }, context))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    collation_name: String,
}

#[cfg(feature = "sqlite")]
impl CollateColumnConstraint {
    const COLLATION_NAMES: [&'static str; 3] = ["BINARY", "NOCASE", "RTRIM"];

    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        _table_name: &TableName,
        syn: &crate::syn::sqlite::CollateColumnConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let collation_name = syn.collation_name().value();
        if !Self::COLLATION_NAMES.contains(&collation_name.to_uppercase().as_str()) {
            context.report_problem(AnalysisProblem::collation_name_not_found(collation_name))?;
        }
        Ok((
            Self {
                collation_name: String::from(collation_name),
            },
            context,
        ))
    }
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ColumnConstraint {
    PrimaryKey(PrimaryKeyColumnConstraint),
    NotNull(ColumnNullableConstraint),
    Unique(UniqueColumnConstraint),
    Check(CheckColumnConstraint),
    Default(DefaultColumnConstraint),
    Collate(CollateColumnConstraint),
}

impl ColumnConstraint {
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        syn: &crate::syn::Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(con_type) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };

        let constraint = match con_type {
            ConstrType::Undefined => todo!(),
            ConstrType::ConstrNull | ConstrType::ConstrNotnull => {
                let (null_constraint, new_context) =
                    ColumnNullableConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::NotNull(null_constraint)
            }
            ConstrType::ConstrDefault => {
                let (c, new_context) =
                    DefaultColumnConstraint::analyze_constraint(context, table_name, syn, tokens)?;
                context = new_context;
                Self::Default(c)
            }
            ConstrType::ConstrIdentity => todo!(),
            ConstrType::ConstrGenerated => todo!(),
            ConstrType::ConstrCheck => {
                let (c, new_context) =
                    CheckColumnConstraint::analyze_constraint(context, table_name, syn, tokens)?;
                context = new_context;
                Self::Check(c)
            }
            ConstrType::ConstrPrimary => {
                let (pk_constraint, new_context) =
                    PrimaryKeyColumnConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::PrimaryKey(pk_constraint)
            }
            ConstrType::ConstrUnique => {
                let (uniq_constraint, new_context) =
                    UniqueColumnConstraint::analyze_constraint(context, table_name, syn)?;
                context = new_context;
                Self::Unique(uniq_constraint)
            }
            ConstrType::ConstrExclusion => todo!(),
            ConstrType::ConstrForeign => todo!(),
            ConstrType::ConstrAttrDeferrable => todo!(),
            ConstrType::ConstrAttrNotDeferrable => todo!(),
            ConstrType::ConstrAttrDeferred => todo!(),
            ConstrType::ConstrAttrImmediate => todo!(),
            ConstrType::OptionNone => todo!(),
        };
        Ok((constraint, context))
    }
}

impl ColumnConstraint {
    #[cfg(feature = "sqlite")]
    pub fn analyze_sqlite_column_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        syn: &crate::syn::sqlite::ColumnConstraint,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::syn::sqlite::ColumnConstraint;

        let result = match syn {
            ColumnConstraint::PrimaryKey(primary_key_column_constraint) => Self::PrimaryKey(
                PrimaryKeyColumnConstraint::from(primary_key_column_constraint),
            ),
            ColumnConstraint::NotNull(not_null_column_constraint) => {
                Self::NotNull(ColumnNullableConstraint::from(not_null_column_constraint))
            }
            ColumnConstraint::Unique(unique_column_constraint) => {
                Self::Unique(UniqueColumnConstraint::from(unique_column_constraint))
            }
            ColumnConstraint::Check(check_column_constraint) => {
                let (c, new_conext) = CheckColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    check_column_constraint,
                    tokens,
                )?;
                context = new_conext;
                Self::Check(c)
            }
            ColumnConstraint::Default(default_column_constraint) => {
                let (c, new_context) = DefaultColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    default_column_constraint,
                    tokens,
                )?;
                context = new_context;
                Self::Default(c)
            }
            ColumnConstraint::Collate(collate_column_constraint) => {
                let (c, new_context) = CollateColumnConstraint::analyze_constraint_sqlite(
                    context,
                    table_name,
                    collate_column_constraint,
                    tokens,
                )?;
                context = new_context;
                Self::Collate(c)
            }
            ColumnConstraint::ForeignKey(_foreign_key_column_constraint) => {
                AnalysisError::raise_unexpected_input(
                    "sqlite foreign key column constraints are not supported yet",
                )?
            }
            ColumnConstraint::Generated(_generated_column_constraint) => {
                AnalysisError::raise_unexpected_input(
                    "sqlite generated column constraints are not supported yet",
                )?
            }
        };
        Ok((result, context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_column_definition {
    use sql_fun_core::IVec;

    use super::{
        CollateColumnConstraint, ColumnConstraint, ColumnDefinition, DefaultColumnConstraint,
    };
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
        let sql = "create table users (id integer primary key not null unique, name text default ('guest') collate nocase, active integer check (true and true), created_at text default current_timestamp)";
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
        assert!(matches!(
            id_col.constraints()[0],
            ColumnConstraint::PrimaryKey(_)
        ));
        assert!(matches!(
            id_col.constraints()[1],
            ColumnConstraint::NotNull(_)
        ));
        assert!(matches!(
            id_col.constraints()[2],
            ColumnConstraint::Unique(_)
        ));
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
            ColumnConstraint::Default(DefaultColumnConstraint {
                expr: SemScalarExpr::Const(ScalarConstExpr::String(value)),
            }) => assert_eq!(value, "guest"),
            _ => panic!("expected string default constraint"),
        }
        match &name_col.constraints()[1] {
            ColumnConstraint::Collate(CollateColumnConstraint { collation_name }) => {
                assert_eq!(collation_name, "nocase")
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
                assert!(matches!(check_constraint.expr, SemScalarExpr::Boolean(_)));
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
            ColumnConstraint::Default(DefaultColumnConstraint {
                expr: SemScalarExpr::SqlValue(SqlValueExpr::CurrentTimestamp(None)),
            }) => {}
            _ => panic!("expected current_timestamp default constraint"),
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
            ColumnConstraint::Collate(CollateColumnConstraint { collation_name }) => {
                assert_eq!(collation_name, "custom")
            }
            _ => panic!("expected collate constraint"),
        }
        assert_eq!(context.reported_problem_count(), 1);
    }
}
