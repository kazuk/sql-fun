use crate::sem::convert;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::AlterTable as SqliteAlterTable;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::AlterTableBody;
use crate::{
    ast::{AlterCommand, AlterRelation},
    sem::{AlterColumnAttributes, ColumnRename, CreateTable, OwnerSpecification},
    syn::{AlterTableCmd, AlterTableStmt},
};
use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::SqliteToken;

use crate::{
    ast::TryFromWithTokens,
    context::GetSourcePathWithSpan,
    names::{ColumnName, ViewName},
    sem::{
        AstAndContextPair, ColumnDefinition, Constraint, ConstraintNames, SemScalarExpr,
        SourcePathWithSpan,
    },
    syn::{AlterTableType, ScanToken},
};

use crate::{
    names::TableName,
    sem::{AnalysisError, ParseContext},
};

/// analyzed alter table statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterTable {
    table: TableName,
    commands: Vec<AlterObjSubCommand>,
    source_span: SourcePathWithSpan,
    #[getter(skip)]
    applicable: bool,
    #[getter(skip)]
    resolved_target: Option<TableName>,
}

impl GetSourcePathWithSpan for AlterTable {
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan {
        &self.source_span
    }
}

impl AlterTable {
    pub(crate) fn column_rename(
        table: &TableName,
        target: Option<&TableName>,
        rename: &ColumnRename,
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(
            table,
            &[AlterObjSubCommand::analyzed_column_rename(rename)],
            source,
        );
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some() && rename.can_apply();
        statement
    }
    /// Completed standalone column rename, including rejected operations.
    pub fn column_rename_change(&self) -> Option<&ColumnRename> {
        let [command] = self.commands.as_slice() else {
            return None;
        };
        command.column_rename_change()
    }

    /// Whether completed semantic analysis permits catalog application.
    pub const fn can_apply(&self) -> bool {
        self.applicable
    }
    /// Canonical target binding retained by semantic analysis.
    pub const fn resolved_target(&self) -> Option<&TableName> {
        self.resolved_target.as_ref()
    }
    pub(crate) fn table_rename(
        table: &TableName,
        target: Option<&TableName>,
        destination: &TableName,
        permitted: bool,
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(table, &[AlterObjSubCommand::rename_to(destination)], source);
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some() && permitted;
        statement
    }

    /// Destination identity retained for a standalone table rename.
    pub const fn rename_destination(&self) -> Option<&TableName> {
        let [command] = self.commands.as_slice() else {
            return None;
        };
        command.rename_destination()
    }

    pub(crate) fn column_attributes(
        table: &TableName,
        target: Option<&TableName>,
        commands: &[AlterObjSubCommand],
        source: &SourcePathWithSpan,
    ) -> Self {
        let mut statement = Self::new(table, commands, source);
        statement.resolved_target = target.cloned();
        statement.applicable = target.is_some()
            && commands
                .iter()
                .all(AlterObjSubCommand::can_apply_column_attributes);
        statement
    }
    pub(crate) fn validate_check_names(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut names = ConstraintNames::default();
        let mut reported = ConstraintNames::default();
        for command in &self.commands {
            if let Some(constraint) = command.added_constraint() {
                let name = constraint.name();
                names.reserve(name);
            }
        }
        for command in &mut self.commands {
            if let AlterObjSubCommand::AddConstraint(Constraint::Check(check)) = command
                && names.is_duplicate(check.name())
                && reported.reserve(check.name())
            {
                check.reject_duplicate_name(context)?;
                self.applicable = false;
            }
        }
        Ok(())
    }

    pub(crate) fn conflicting_constraint(&self, table: &CreateTable) -> Option<String> {
        let mut names = ConstraintNames::default();
        for command in &self.commands {
            if let Some(constraint) = command.added_constraint() {
                let name = constraint.name();
                if !name.is_empty() && (table.constraint(name).is_some() || !names.reserve(name)) {
                    return Some(name.to_owned());
                }
            }
        }
        None
    }

    pub(crate) fn applied_owner(&self) -> Option<&OwnerSpecification> {
        self.commands
            .iter()
            .rev()
            .find_map(AlterObjSubCommand::applied_owner)
    }

    pub(crate) fn new(
        table: &TableName,
        commands: &[AlterObjSubCommand],
        source_span: &SourcePathWithSpan,
    ) -> Self {
        Self {
            table: table.clone(),
            commands: commands.to_vec(),
            source_span: source_span.clone(),
            applicable: commands
                .iter()
                .all(AlterObjSubCommand::declaration_can_apply),
            resolved_target: None,
        }
    }
    /// analyze [`crate::syn::AlterTableStmt`]
    pub fn analyze_alter_table_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterRelation::try_from_with_tokens(&syn, tokens)?)
            .resolve_and_apply_at(context, source_span)
    }

    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterRelation::try_from_with_tokens(&syn, tokens)?)
            .resolve_and_apply_at(context, source_span)
    }

    /// analyze [`crate::syn::sqlite::AlterTable`]
    #[cfg(feature = "sqlite")]
    #[tracing::instrument(skip(context, syn))]
    pub fn analyze_alter_table<TParseContext>(
        context: TParseContext,
        syn: SqliteAlterTable,
        tokens: &IVec<SqliteToken<'_>>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterRelation::try_from(&syn)?).resolve_and_apply_at(context, source_span)
    }
}

#[cfg(test)]
mod test_analyze_alter_table_statement {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::alter_table::{make_column, parse_sqlite_alter_table, sqlite_context};

    #[cfg(feature = "sqlite")]
    use crate::ast::Dialect;

    use crate::{
        AstAndContextPair,
        names::FullName,
        sem::{AlterTable, SemAst, SourcePathWithSpan},
        syn::{AliasOpt, NodeInner, ObjectType},
        test_helpers::{SynBuilder, TestParseContext, source_span, test_context},
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
        let AstAndContextPair(res, ctx) = AlterTable::analyze_alter_table_statement(
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
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_missing_relation(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) = parse_sqlite_alter_table("alter table users rename to members");
        let context = TestParseContext::default().with_dialect(Dialect::sqlite());

        let AstAndContextPair(res, context) =
            AlterTable::analyze_alter_table(context, alter_table, &tokens, &source_span)
                .expect("sqlite alter table should analyze");
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands.len(), 1);
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    fn test_analyze_sqlite_alter_table_reports_existing_relation(source_span: SourcePathWithSpan) {
        let (alter_table, tokens) = parse_sqlite_alter_table("alter table users rename to members");
        let mut context = sqlite_context();
        context.setup_table("users", &[], &source_span);
        context.setup_table("members", &[], &source_span);

        let AstAndContextPair(_, context) =
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

        let AstAndContextPair(_, context) =
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

        let AstAndContextPair(_, context) =
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

        let AstAndContextPair(_, context) =
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

        let AstAndContextPair(_, context) =
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
        syn: AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(
            &AlterRelation::try_from_with_tokens(&syn, &IVec::default())?
                .with_view_kind(as_matview),
        )
        .resolve_and_apply(context)
    }
}

#[derive(Debug, Clone)]
pub enum AlterObjSubCommand {
    /// An analyzed PostgreSQL column-attribute operation.
    ColumnAttributes(Box<AlterColumnAttributes>),
    ChangeOwner(OwnerSpecification),
    SetColumnDefault(ColumnName, Box<SemScalarExpr>),
    /// Remove a column default.
    DropColumnDefault(ColumnName),
    AddConstraint(Constraint),
    ClusterOn(String),

    RenameTo(TableName),
    AddColumn(ColumnDefinition),
    AlterColumn(ColumnName, ColumnDefinition),
    RenameColumn(Box<ColumnRename>),
    DropColumn(ColumnName),
}

impl AlterObjSubCommand {
    fn declaration_can_apply(&self) -> bool {
        match self {
            Self::AddConstraint(constraint) => {
                constraint.as_check().is_none_or(|check| check.can_apply())
            }
            Self::RenameColumn(rename) => rename.can_apply(),
            Self::AddColumn(column) | Self::AlterColumn(_, column) => {
                column.declaration_can_apply()
            }
            _ => true,
        }
    }

    const fn rename_destination(&self) -> Option<&TableName> {
        if let Self::RenameTo(name) = self {
            Some(name)
        } else {
            None
        }
    }

    pub(crate) fn column_attributes(value: &AlterColumnAttributes) -> Self {
        Self::ColumnAttributes(Box::new(value.clone()))
    }
    /// Retained column binding, requested action, and validation outcomes.
    pub fn column_attribute_change(&self) -> Option<&AlterColumnAttributes> {
        if let Self::ColumnAttributes(value) = self {
            Some(value)
        } else {
            None
        }
    }
    fn can_apply_column_attributes(&self) -> bool {
        self.column_attribute_change()
            .is_some_and(AlterColumnAttributes::can_apply)
    }
    pub(crate) fn apply_column_attributes(&self, table: &mut CreateTable) {
        if let Self::ColumnAttributes(value) = self {
            value.apply(table);
        }
    }
    const fn added_constraint(&self) -> Option<&Constraint> {
        if let Self::AddConstraint(constraint) = self {
            Some(constraint)
        } else {
            None
        }
    }

    const fn applied_owner(&self) -> Option<&OwnerSpecification> {
        if let Self::ChangeOwner(owner) = self
            && owner.can_apply()
        {
            Some(owner)
        } else {
            None
        }
    }

    pub(crate) fn resolved_owner(owner: &OwnerSpecification) -> Self {
        Self::ChangeOwner(owner.clone())
    }
    pub(crate) fn drop_column_default(column: &ColumnName) -> Self {
        Self::DropColumnDefault(column.clone())
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
    pub(crate) fn analyzed_column_rename(rename: &ColumnRename) -> Self {
        Self::RenameColumn(Box::new(rename.clone()))
    }
    /// Completed column-rename semantics.
    pub fn column_rename_change(&self) -> Option<&ColumnRename> {
        if let Self::RenameColumn(rename) = self {
            Some(rename)
        } else {
            None
        }
    }
    pub(crate) fn drop_column(column: &ColumnName) -> Self {
        Self::DropColumn(column.clone())
    }
}

#[cfg(test)]
pub fn analyze_alter_obj_change_owner<TParseContext>(
    _context: &TParseContext,
    cmd: AlterTableCmd,
) -> Result<AlterObjSubCommand, AnalysisError> {
    let Some(new_owner) = cmd.get_newowner().get_rolename() else {
        AnalysisError::raise_unexpected_none("alter_table_cmd.newwoner")?
    };
    Ok(AlterObjSubCommand::resolved_owner(
        &OwnerSpecification::named(&new_owner),
    ))
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
        cmd: AlterTableCmd,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterCommand::try_from_with_tokens(&cmd, tokens)?)
            .resolve_and_report(context, table_name)
    }
}

#[cfg(test)]
mod test_analyze_alter_obj_column_default {
    use crate::{names::ColumnName, sem::ColumnDefinition};

    use crate::{
        names::{FullName, TableName},
        sem::{
            ScalarConstExpr, SemScalarExpr, SourcePathWithSpan, alter_table::AlterObjSubCommand,
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
            &[ColumnDefinition::new(
                &Some(ColumnName::from("value")),
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
        body: &AlterTableBody,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterCommand::try_from(body)?).resolve_and_report(context, table_name)
    }
}

impl AlterObjSubCommand {
    pub fn analyze_alter_table_cmd<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        cmd: AlterTableCmd,
        _subtyp: AlterTableType,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        convert(&AlterCommand::try_from_with_tokens(&cmd, tokens)?)
            .resolve_and_report(context, table_name)
    }
}

#[cfg(test)]
mod test_column_attribute_command {
    //! SQL-to-semantics and catalog-chain regressions; no external server is required.

    use crate::{
        sem::AlterObjSubCommand,
        syn::{ListOpt, Opt},
        test_helpers::SynBuilder,
    };

    use crate::test_helpers::column_attributes as test_helpers;

    #[test]
    fn standalone_semantic_command_api_resolves_column_attributes() -> testresult::TestResult {
        let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
        let node = SynBuilder::new().parse("ALTER TABLE t ALTER a SET STATISTICS 42");
        let statement = node.as_alter_table_stmt().as_inner().unwrap();
        let commands = statement.get_cmds().as_inner().unwrap();
        let command = commands[0].as_alter_table_cmd().as_inner().unwrap();
        let subtype = command.get_subtype().as_inner().unwrap();
        let (command, context) = AlterObjSubCommand::analyze_alter_table_cmd(
            context,
            &test_helpers::table_name(),
            command,
            subtype,
            &::sql_fun_core::IVec::default(),
        )?;
        let change = command.column_attribute_change().unwrap();
        assert!(change.can_apply());
        assert_eq!(
            change.action().statistics_target().unwrap().effective(),
            Some(42)
        );
        assert!(change.binding().definition().is_some());
        assert!(context.problems().is_empty());
        // Command analysis returns semantics without applying the enclosing DDL.
        assert_eq!(
            test_helpers::column(&context, "a").statistics_target(),
            None
        );
        Ok(())
    }
}
