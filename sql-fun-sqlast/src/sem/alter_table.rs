use sql_fun_core::IVec;

use crate::{
    sem::{
        AlterSequence, AstAndContextPair, ColumnDefinition, ColumnName, Constraint, FromClause,
        GetSourcePathWithSpan, SemScalarExpr, SourcePathWithSpan, ViewName, WithClause,
    },
    syn::{AlterTableType, ListOpt, ObjectType, Opt, ScanToken},
};

use super::{AnalysisError, AnalysisProblem, ParseContext, SemAst, create_table::TableName};

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
    /// analyze [`crate::syn::AlterTableStmt`]
    pub fn analyze_alter_table_statement<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(rel) = syn.get_relation().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
        };
        let table_name = TableName::try_from(rel)?;
        if context.get_table(&table_name).is_none() {
            context.report_problem(AnalysisProblem::relation_not_found(&table_name))?;
        }

        let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
        };
        let mut sub_cmds = Vec::new();
        for cmd in cmds {
            let Some(subtyp) = cmd.get_subtype().as_inner() else {
                AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
            };
            let (sub_cmd, new_context) = AlterObjSubCommand::analyze_alter_table_cmd(
                context,
                &table_name,
                cmd,
                subtyp,
                tokens,
            )?;
            context = new_context;
            sub_cmds.push(sub_cmd);
        }
        let alter_table = Self {
            table: table_name,
            commands: sub_cmds,
            source_span: source_span.clone(),
        };
        context = context.apply_alter_table(&alter_table)?;
        Ok(AstAndContextPair::new(
            SemAst::AlterTable(alter_table),
            context,
        ))
    }

    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, parent_schema, syn))]
    pub fn analyze_alter_statement<TParseContext>(
        context: TParseContext,
        parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(obj_type) = syn.get_objtype().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.objtype")?
        };
        match obj_type {
            ObjectType::ObjectTable => Self::analyze_alter_table_statement(
                context,
                parent_schema,
                syn,
                tokens,
                source_span,
            ),
            ObjectType::ObjectView => {
                AlterView::analyze_alter_view_statement(context, parent_schema, false, syn)
            }
            ObjectType::ObjectSequence => {
                AlterSequence::analyze_alter_seq_statement(context, parent_schema, syn)
            }
            ObjectType::ObjectMatview => {
                AlterView::analyze_alter_view_statement(context, parent_schema, true, syn)
            }
            _ => todo!("{obj_type:?}"),
        }
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
            builder.alter_table_stmt(relation.into(), cmds, ObjectType::ObjectTable.into());
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
}

/// analyzed alter view statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterView {
    view: ViewName,
    commands: Vec<AlterObjSubCommand>,
}

impl AlterView {
    /// analyze [`crate::syn::AlterTableStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_alter_view_statement<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        as_matview: bool,
        syn: crate::syn::AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(rel) = syn.get_relation().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
        };
        let view_name = ViewName::from(rel);
        if let Some(view) = context.get_view(&view_name) {
            if as_matview && !view.is_materialized() {
                context.report_problem(AnalysisProblem::view_not_matview(&view_name))?;
            }
        } else {
            context.report_problem(AnalysisProblem::view_not_found(&view_name))?;
        }

        let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
        };
        let mut sub_cmds = Vec::new();
        for cmd in cmds {
            let Some(subtyp) = cmd.get_subtype().as_inner() else {
                AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
            };
            let sub_cmd = match subtyp {
                AlterTableType::AtChangeOwner => analyze_alter_obj_change_owner(&context, cmd)?,

                _ => todo!("{subtyp:?}"),
            };
            sub_cmds.push(sub_cmd);
        }
        let alter_view = Self {
            view: view_name,
            commands: sub_cmds,
        };

        context = context.apply_alter_view(&alter_view)?;
        Ok(AstAndContextPair::new(
            SemAst::AlterView(alter_view),
            context,
        ))
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

fn report_missing_column_in_table<TParseContext>(
    context: &mut TParseContext,
    table_name: &TableName,
    column_name: &ColumnName,
) -> Result<(), AnalysisError>
where
    TParseContext: ParseContext,
{
    if let Some(table) = context.get_table(table_name)
        && !table.has_column(column_name)
    {
        context.report_problem(AnalysisProblem::column_not_found_in_table(
            table_name,
            column_name,
        ))?;
    }
    Ok(())
}

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
    fn analyze_alter_obj_column_default<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        cmd: crate::syn::AlterTableCmd,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let column_name = ColumnName::from(cmd.get_name().clone());
        let with_clause = WithClause::default();
        let from_clause = FromClause::default();
        let (expr, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &with_clause,
            &from_clause,
            cmd.get_def(),
            tokens,
        )?;
        context = new_context;

        report_missing_column_in_table(&mut context, table_name, &column_name)?;

        Ok((Self::SetColumnDefault(column_name, Box::new(expr)), context))
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

impl AlterObjSubCommand {
    pub fn analyze_alter_table_cmd<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        cmd: crate::syn::AlterTableCmd,
        subtyp: AlterTableType,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let sub_cmd = match subtyp {
            AlterTableType::AtChangeOwner => analyze_alter_obj_change_owner(&context, cmd)?,
            AlterTableType::AtColumnDefault => {
                let (column_default, new_context) =
                    Self::analyze_alter_obj_column_default(context, table_name, cmd, tokens)?;
                context = new_context;
                column_default
            }
            AlterTableType::AtAddConstraint => {
                Constraint::analyze_alter_obj_add_constraint(&mut context, table_name, cmd)?
            }
            AlterTableType::AtClusterOn => {
                let index_name = cmd.get_name();
                if context.get_index(&index_name).is_none() {
                    context.report_problem(AnalysisProblem::index_not_found(&index_name))?;
                }
                Self::ClusterOn(cmd.get_name())
            }
            AlterTableType::AtAddColumn => {
                todo!()
            }
            AlterTableType::AtDropColumn => {
                todo!()
            }

            _ => todo!("{subtyp:?}"),
        };
        Ok((sub_cmd, context))
    }
}

#[deprecated = "use AlterObjSubCommand::analyze_alter_table_cmd"]
#[expect(dead_code)]
pub fn analyze_alter_table_cmd<TParseContext>(
    context: TParseContext,
    table_name: &TableName,
    cmd: crate::syn::AlterTableCmd,
    subtyp: AlterTableType,
    tokens: &IVec<ScanToken>,
) -> Result<(AlterObjSubCommand, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    AlterObjSubCommand::analyze_alter_table_cmd(context, table_name, cmd, subtyp, tokens)
}

#[deprecated = "use AlterView::analyze_alter_view_statement"]
#[expect(dead_code)]
pub fn analyze_alter_view_statement<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    as_matview: bool,
    syn: crate::syn::AlterTableStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    AlterView::analyze_alter_view_statement(context, parent_schema, as_matview, syn)
}

#[deprecated = "use AlterSequence::analyze_alter_seq_statement"]
#[expect(dead_code)]
pub fn analyze_alter_seq_statement<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::AlterTableStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    AlterSequence::analyze_alter_seq_statement(context, parent_schema, syn)
}
