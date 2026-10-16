use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;

use crate::{
    ast::TryFromWithTokens,
    sem::{
        AstAndContextPair, ColumnDefinition, ColumnName, Constraint, GetSourcePathWithSpan,
        SemScalarExpr, SourcePathWithSpan, ViewName,
    },
    syn::{AlterTableType, ScanToken},
};

use super::{AnalysisError, ParseContext, create_table::TableName};

/// analyzed alter table statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterTable {
    table: TableName,
    commands: Vec<AlterObjSubCommand>,
    source_span: SourcePathWithSpan,
}

impl GetSourcePathWithSpan for AlterTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source_span
    }
}

impl AlterTable {
    pub(crate) fn new(
        table: &TableName,
        commands: &[AlterObjSubCommand],
        source_span: &SourcePathWithSpan,
    ) -> Self {
        Self {
            table: table.clone(),
            commands: commands.to_vec(),
            source_span: source_span.clone(),
        }
    }
    /// analyze [`crate::syn::AlterTableStmt`]
    pub fn analyze_alter_table_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterRelation::try_from_with_tokens(&syn, tokens)?.resolve_and_apply_at(
            context,
            crate::ast::Dialect::postgres(),
            source_span,
        )
    }

    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterRelation::try_from_with_tokens(&syn, tokens)?.resolve_and_apply_at(
            context,
            crate::ast::Dialect::postgres(),
            source_span,
        )
    }

    /// analyze [`crate::syn::sqlite::AlterTable`]
    #[cfg(feature = "sqlite")]
    #[tracing::instrument(skip(context, syn))]
    pub fn analyze_alter_table<TParseContext>(
        context: TParseContext,
        syn: crate::syn::sqlite::AlterTable,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterRelation::try_from(&syn)?.resolve_and_apply_at(
            context,
            crate::ast::Dialect::sqlite(),
            source_span,
        )
    }
}

#[cfg(test)]
mod test_analyze_alter_table_statement {
    use crate::{
        AstAndContextPair,
        sem::{AlterTable, FullName, SemAst, SourcePathWithSpan},
        syn::{AliasOpt, NodeInner, ObjectType},
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };

    #[cfg(feature = "sqlite")]
    use crate::{
        SqliteToken,
        sem::{ColumnDefinition, ColumnName, SchemaName, TypeReference},
        sqlite::SqliteDialect,
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_table_statement(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        test_context.setup_table("table1", &[], &source_span);

        let builder = SynBuilder::new();
        let relation = builder.range_var(
            &FullName::with_schema("public", "table1"),
            &AliasOpt::none(),
        );
        let cmd = builder.alter_table_cmd_set_column_default(
            "col1",
            NodeInner::AConst(builder.const_int4(10)).into(),
        );
        let cmds = builder.as_node_list(&[NodeInner::AlterTableCmd(cmd).into()]);
        let alter_table_stmt =
            builder.alter_table_stmt(relation.into(), cmds, ObjectType::ObjectTable);
        let tokens = IVec::default();
        let AstAndContextPair(res, ctx, _) = AlterTable::analyze_alter_table_statement(
            test_context,
            &None,
            alter_table_stmt,
            &tokens,
            &source_span,
        )?;
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands.len(), 1);
        assert_eq!(ctx.alter_tables_len(), 1);
        assert_eq!(ctx.reported_problem_count(), 1);
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    fn parse_sqlite_alter_table<'a>(
        sql: &'a str,
    ) -> (crate::syn::sqlite::AlterTable, IVec<SqliteToken<'a>>) {
        let cmd = SqliteDialect::parse(sql)
            .expect("sqlite parse should succeed")
            .expect("statement should exist");
        let tokens = SqliteDialect::scan(sql).expect("sqlite scan should succeed");
        let stmt = cmd.into_plain_statement().expect("plain SQLite statement");
        let alter_table = stmt.into_alter_table().expect("ALTER TABLE statement");
        (alter_table, tokens)
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_missing_relation(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) = parse_sqlite_alter_table("alter table users rename to members");
        let context = TestParseContext::default();

        let AstAndContextPair(res, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands.len(), 1);
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    fn make_column(name: &str) -> ColumnDefinition {
        ColumnDefinition::new(&Some(ColumnName::from(name)), None, None, &[])
    }

    #[cfg(feature = "sqlite")]
    fn sqlite_context() -> TestParseContext {
        let mut context = TestParseContext::default();
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context.setup_type(TypeReference::concrete_type_ref(
            &FullName::with_schema("public", "text"),
            false,
        ));
        context
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_existing_relation(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) = parse_sqlite_alter_table("alter table users rename to members");
        let mut context = sqlite_context();
        context.setup_table("users", &[], &source_span);
        context.setup_table("members", &[], &source_span);

        let AstAndContextPair(_, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_existing_column(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) =
            parse_sqlite_alter_table("alter table users add column name text");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_missing_column(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) =
            parse_sqlite_alter_table("alter table users rename column missing to renamed");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_rename_target_column_exists(
        source_span: SourcePathWithSpan,
    ) {
        let (alter_table, tokens) =
            parse_sqlite_alter_table("alter table users rename column name to email");
        let mut context = sqlite_context();
        context.setup_table(
            "users",
            &[make_column("name"), make_column("email")],
            &source_span,
        );

        let AstAndContextPair(_, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_drop_missing_column(
        source_span: SourcePathWithSpan,
    ) {
        let (alter_table, tokens) =
            parse_sqlite_alter_table("alter table users drop column missing");
        let mut context = sqlite_context();
        context.setup_table("users", &[make_column("name")], &source_span);

        let AstAndContextPair(_, context, _) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        assert_eq!(context.reported_problem_count(), 1);
    }
}

/// analyzed alter view statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterView {
    view: ViewName,
    commands: Vec<AlterObjSubCommand>,
}

impl AlterView {
    pub(crate) fn new(view: &ViewName, commands: &[AlterObjSubCommand]) -> Self {
        Self {
            view: view.clone(),
            commands: commands.to_vec(),
        }
    }
    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_view_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        as_matview: bool,
        syn: crate::syn::AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterRelation::try_from_with_tokens(&syn, &IVec::default())?
            .with_view_kind(as_matview)
            .resolve_and_apply(context, crate::ast::Dialect::postgres())
    }
}

#[derive(Debug, Clone)]
pub enum AlterObjSubCommand {
    ChangeOwner(String),
    SetColumnDefault(ColumnName, Box<SemScalarExpr>),
    AddConstraint(Constraint),
    ClusterOn(String),

    RenameTo(TableName),
    AddColumn(ColumnDefinition),
    AlterColumn(ColumnName, ColumnDefinition),
    RenameColumn(ColumnName, ColumnName),
    DropColumn(ColumnName),
}

impl AlterObjSubCommand {
    pub(crate) fn change_owner(owner: &str) -> Self {
        Self::ChangeOwner(owner.to_owned())
    }
    pub(crate) fn column_default(column: &ColumnName, expression: &SemScalarExpr) -> Self {
        Self::SetColumnDefault(column.clone(), Box::new(expression.clone()))
    }
    pub(crate) fn add_constraint(constraint: &Constraint) -> Self {
        Self::AddConstraint(constraint.clone())
    }
    pub(crate) fn cluster_on(index: &str) -> Self {
        Self::ClusterOn(index.to_owned())
    }
    pub(crate) fn rename_to(table: &TableName) -> Self {
        Self::RenameTo(table.clone())
    }
    pub(crate) fn add_column(column: &ColumnDefinition) -> Self {
        Self::AddColumn(column.clone())
    }
    pub(crate) fn alter_column(name: &ColumnName, column: &ColumnDefinition) -> Self {
        Self::AlterColumn(name.clone(), column.clone())
    }
    pub(crate) fn rename_column(from: &ColumnName, to: &ColumnName) -> Self {
        Self::RenameColumn(from.clone(), to.clone())
    }
    pub(crate) fn drop_column(column: &ColumnName) -> Self {
        Self::DropColumn(column.clone())
    }
}

#[cfg(test)]
pub fn analyze_alter_obj_change_owner<TParseContext>(
    _context: &TParseContext,
    cmd: crate::syn::AlterTableCmd,
) -> Result<AlterObjSubCommand, AnalysisError> {
    let Some(new_owner) = cmd.get_newowner().get_rolename() else {
        AnalysisError::raise_unexpected_none("alter_table_cmd.newwoner")?
    };
    Ok(AlterObjSubCommand::ChangeOwner(new_owner))
}

#[cfg(test)]
mod test_analyze_alter_obj_change_owner {
    use crate::{
        sem::alter_table::analyze_alter_obj_change_owner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_change_owner(test_context: TestParseContext) -> TestResult {
        let alter_table_cmd = SynBuilder::new().alter_table_cmd_change_owner("test");
        let _result = analyze_alter_obj_change_owner(&test_context, alter_table_cmd)?;
        Ok(())
    }
}

impl AlterObjSubCommand {
    #[cfg(test)]
    fn analyze_alter_obj_column_default<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        cmd: crate::syn::AlterTableCmd,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterCommand::try_from_with_tokens(&cmd, tokens)?.resolve_and_report(
            context,
            crate::ast::Dialect::postgres(),
            table_name,
        )
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_column_default {
    use crate::{
        sem::{
            FullName, ScalarConstExpr, SemScalarExpr, SourcePathWithSpan, TableName,
            alter_table::AlterObjSubCommand,
        },
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_column_default(
        mut test_context: TestParseContext,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        // alter table ...
        //   alter column value set default(0)
        test_context.setup_table(
            "table1",
            &[crate::sem::ColumnDefinition::new(
                &Some(crate::sem::ColumnName::from("value")),
                None,
                None,
                &[],
            )],
            &source_span,
        );

        let builder = SynBuilder::new();
        let value = builder.const_int4(0);
        let cmd =
            builder.alter_table_cmd_set_column_default("value", NodeInner::AConst(value).into());
        let tokens = IVec::default();
        let table_name = TableName::from(FullName::with_schema("public", "table1"));

        let (res, test_context) = AlterObjSubCommand::analyze_alter_obj_column_default(
            test_context,
            &table_name,
            cmd,
            &tokens,
        )?;
        let AlterObjSubCommand::SetColumnDefault(col_name, expr) = res else {
            panic!("unexpected response {res:#?}");
        };
        assert_eq!(col_name.as_str(), "value");
        let SemScalarExpr::Const(ScalarConstExpr::Integer(val)) = *expr else {
            panic!("unexpected expr {expr:#?}");
        };
        assert_eq!(val, 0);
        assert_eq!(test_context.reported_problem_count(), 0);

        Ok(())
    }
}

#[cfg(feature = "sqlite")]
impl AlterObjSubCommand {
    pub fn analyze_alter_table_body<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        body: &crate::syn::sqlite::AlterTableBody,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterCommand::try_from(body)?.resolve_and_report(
            context,
            crate::ast::Dialect::sqlite(),
            table_name,
        )
    }
}

impl AlterObjSubCommand {
    pub fn analyze_alter_table_cmd<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        cmd: crate::syn::AlterTableCmd,
        _subtyp: AlterTableType,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterCommand::try_from_with_tokens(&cmd, tokens)?.resolve_and_report(
            context,
            crate::ast::Dialect::postgres(),
            table_name,
        )
    }
}
