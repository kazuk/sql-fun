#[cfg(feature = "sqlite")]
use crate::syn::sqlite::ColumnDefinition as SqliteColumnDefinition;
use crate::{
    ast::Column,
    names::FullName as NamesFullName,
    sem::{
        CatalogNameLookup, ColumnNullabilityConstraint, ColumnRename, DeclarationCollation,
        DefaultColumnConstraint, GeneratedColumnConstraint, WithTypmod,
        traversal::{ColumnRenamer, SemanticVisitorMut},
    },
    syn::ColumnDef,
};
use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    names::{ColumnName, TableName},
    sem::{AnalysisError, ColumnConstraint, ParseContext, SemScalarExpr, TypeReference},
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
    #[serde(default, skip_serializing_if = "ColumnAttributes::is_empty")]
    attributes: ColumnAttributes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    declaration: Option<Box<ColumnDeclaration>>,
}

// Keep declaration-only metadata out of the inline size of every column binding.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct ColumnDeclaration {
    requested_type: Option<WithTypmod<TypeReference>>,
    collation: DeclarationCollation,
}

/// Persistent PostgreSQL per-column planning attributes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct ColumnAttributes {
    statistics_target: Option<i32>,
    n_distinct: Option<String>,
    n_distinct_inherited: Option<String>,
}
impl ColumnAttributes {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl ColumnDefinition {
    pub(crate) fn set_projection_name(&mut self, name: &ColumnName) {
        self.name = Some(name.clone());
    }
    pub(crate) fn rename_column_name(&mut self, rename: &ColumnRename) {
        self.rename_constraint_columns(rename, true);
        if let Some(name) = &mut self.name {
            rename.rename_name(name);
        }
    }
    pub(crate) fn rename_column_dependencies(
        &mut self,
        rename: &ColumnRename,
        owned: bool,
    ) -> Result<(), AnalysisError> {
        self.rename_constraint_columns(rename, owned);
        self.rename_expressions(&mut ColumnRenamer::new(rename))
    }
    fn rename_constraint_columns(&mut self, rename: &ColumnRename, owned: bool) {
        for constraint in &mut self.constraints {
            constraint.rename_column(rename, owned);
        }
    }

    /// Requested declaration type and modifiers, separate from canonical identity.
    pub fn requested_type(&self) -> Option<&WithTypmod<TypeReference>> {
        self.declaration.as_ref()?.requested_type.as_ref()
    }

    pub(crate) fn retain_requested_type(&mut self, value: &WithTypmod<TypeReference>) {
        let declaration = self.declaration.get_or_insert_default();
        declaration.requested_type = Some(value.clone());
    }

    /// Retained explicit declaration collation analysis.
    pub fn declaration_collation(&self) -> &DeclarationCollation {
        if let Some(declaration) = &self.declaration {
            &declaration.collation
        } else {
            DeclarationCollation::omitted()
        }
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl ParseContext,
        lookup: Option<&CatalogNameLookup>,
    ) -> Result<(), AnalysisError> {
        let declaration = self.declaration.get_or_insert_default();
        declaration.collation =
            DeclarationCollation::validate(context, self.type_ref.as_ref(), lookup);
        declaration
            .collation
            .report(context, self.type_ref.as_ref())
    }

    pub(crate) fn declaration_can_apply(&self) -> bool {
        let Some(declaration) = &self.declaration else {
            return true;
        };
        // An absent type is not itself a failed declaration (SQLite permits
        // typeless columns, and query-derived columns may remain unknown).
        let type_resolved = declaration.requested_type.is_none() || self.type_ref.is_some();
        type_resolved && declaration.collation.can_apply()
    }

    pub(super) fn rename_expressions(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        for constraint in &mut self.constraints {
            constraint.rename_expression(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        for constraint in &mut self.constraints {
            constraint.rename_relation(old, new);
        }
        if let Some(ty) = &mut self.type_ref {
            ty.rename_relation(old, new);
        }
    }

    /// Explicit statistics target; None uses PostgreSQL's configured default.
    pub const fn statistics_target(&self) -> Option<i32> {
        self.attributes.statistics_target
    }
    /// Explicit n_distinct estimate retained from ALTER COLUMN SET.
    pub fn distinct_estimate(&self) -> Option<f64> {
        if let Some(value) = &self.attributes.n_distinct {
            value.trim().parse().ok()
        } else {
            None
        }
    }
    /// Explicit n_distinct_inherited estimate retained from ALTER COLUMN SET.
    pub fn inherited_distinct_estimate(&self) -> Option<f64> {
        if let Some(value) = &self.attributes.n_distinct_inherited {
            value.trim().parse().ok()
        } else {
            None
        }
    }
    pub(crate) const fn apply_statistics_target(&mut self, value: Option<i32>) {
        self.attributes.statistics_target = value;
    }
    pub(crate) fn apply_distinct_option(&mut self, name: &str, value: Option<&str>) {
        match name {
            "n_distinct" => self.attributes.n_distinct = value.map(str::to_owned),
            "n_distinct_inherited" => {
                self.attributes.n_distinct_inherited = value.map(str::to_owned)
            }
            _ => unreachable!("only validated column options are applied"),
        }
    }
    pub(crate) fn drop_generation_expression(&mut self) {
        self.constraints
            .retain(|constraint| constraint.as_generated().is_none());
    }
    pub(crate) fn apply_attribute_nullability(&mut self, table: &TableName, value: bool) {
        self.constraints
            .retain(|constraint| constraint.as_nullability().is_none());
        if let Some(column) = &self.name {
            self.constraints.push(ColumnConstraint::nullability(
                ColumnNullabilityConstraint::new(table, column, value, None, None),
            ));
        }
        // Keep the existing query metadata accessor in sync with the constraint.
        self.set_not_null(Some(value));
    }
    pub(crate) fn has_identity(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.as_identity().is_some())
    }
    pub(crate) fn has_inline_primary_key(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.as_primary_key().is_some())
    }
    pub(crate) fn has_stored_generation(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint
                .as_generated()
                .is_some_and(GeneratedColumnConstraint::is_stored)
        })
    }
    pub(crate) fn has_virtual_generation(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint
                .as_generated()
                .is_some_and(GeneratedColumnConstraint::is_virtual)
        })
    }
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
            attributes: self.attributes.clone(),
            declaration: self.declaration.clone(),
        }
    }
}

impl ColumnDefinition {
    pub(crate) fn replace_view_default(&mut self, view: &NamesFullName, value: &SemScalarExpr) {
        self.constraints
            .retain(|constraint| constraint.as_default().is_none());
        if let Some(name) = &self.name {
            self.constraints
                .push(ColumnConstraint::default(DefaultColumnConstraint::new(
                    &TableName::from(view.clone()),
                    name,
                    value.clone(),
                )));
        }
    }

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
            attributes: ColumnAttributes::default(),
            declaration: None,
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

    pub(super) fn apply_primary_key_nullability(&mut self) {
        if self
            .constraints
            .iter()
            .any(|constraint| constraint.as_primary_key().is_some())
        {
            self.set_not_null(Some(true));
        }
    }

    pub(super) fn has_unique_key(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint.as_primary_key().is_some() || constraint.as_unique().is_some()
        })
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
        column_def: &ColumnDef,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Column::try_from_with_tokens(column_def, tokens)?.resolve_and_report(context, table_name)
    }

    /// analyze [`crate::syn::sqlite::ColumnDefinition`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_column_definition<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        syn: &SqliteColumnDefinition,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Column::try_from(syn)?.resolve_and_report(context, table_name)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_column_definition {
    use crate::test_helpers::column_definition::setup_context;

    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::CreateTableStatement;

    use assert_matches::assert_matches;
    use sql_fun_core::IVec;

    use crate::{
        SqliteToken,
        names::TableName,
        sem::{
            SemScalarExpr,
            create_table::column_def::{ColumnConstraint, ColumnDefinition},
        },
        sqlite::SqliteDialect,
        syn::sqlite::CreateTableBody,
    };

    fn parse_create_table<'a>(sql: &'a str) -> (CreateTableStatement, IVec<SqliteToken<'a>>) {
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

    #[rstest::fixture]
    fn create_table_statement<'a>() -> (CreateTableStatement, IVec<SqliteToken<'a>>) {
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
        create_table_statement: (CreateTableStatement, IVec<SqliteToken<'a>>),
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
